import { createHash } from "node:crypto";
import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { ESLint } from "eslint";
import * as prettier from "prettier";
import { architectureIssues, baselineGrowth, excessIssues, lineLimit } from "./rules.mjs";

process.chdir(fileURLToPath(new URL("../../", import.meta.url)));
const baselinePath = ".quality-baseline.json";
const args = process.argv.slice(2);
const mode = args[0] ?? "check";
const writeBaseline = args.includes("--write-baseline");
const strict = args.includes("--strict");
const baseIndex = args.indexOf("--base");
if (writeBaseline && process.env.CI) throw new Error("CI must not write the quality baseline");
if (
  !["check", "lint", "format", "architecture", "rust"].includes(mode) ||
  (writeBaseline && mode !== "check") ||
  (baseIndex >= 0 && !args[baseIndex + 1])
) {
  throw new Error(
    "Usage: node scripts/quality/check.mjs [check|lint|format|architecture|rust] [--strict] [--base REF] [--write-baseline]",
  );
}
const git = (...params) => execFileSync("git", params, { encoding: "utf8" }).trim();
const hash = (value) => createHash("sha256").update(value).digest("hex");
const fingerprint = (issue) =>
  hash(JSON.stringify([issue.file, issue.rule, issue.message, issue.evidence]));
const files = [
  ...new Set(
    execFileSync("git", ["ls-files", "-z", "--cached", "--others", "--exclude-standard"], {
      encoding: "utf8",
    })
      .split("\0")
      .filter(Boolean),
  ),
].sort();
const baseline =
  writeBaseline || strict
    ? { version: 1, issues: {}, formatting: {}, lines: {} }
    : JSON.parse(readFileSync(baselinePath, "utf8"));
if (baseline.version !== 1) throw new Error("Unsupported quality baseline version");
const issues = [];
const formatting = {};
const lines = {};
const failures = [];
const sources = new Map();
for (const file of files) {
  if (
    (/^(src\/|src-tauri\/src\/|tests\/frontend\/|scripts\/(?:quality|ci|e2e)\/|tests\/quality\/)/.test(
      file,
    ) &&
      /\.(?:tsx?|rs|mjs)$/.test(file)) ||
    file === "eslint.config.mjs"
  ) {
    try {
      sources.set(file, readFileSync(file, "utf8"));
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
  }
}

if (mode === "check" || mode === "lint") {
  const eslint = new ESLint({ allowInlineConfig: false });
  const reports = await eslint.lintFiles([
    "src",
    "tests/frontend",
    "scripts/quality",
    ...(existsSync("scripts/ci") ? ["scripts/ci"] : []),
    ...(existsSync("scripts/e2e") ? ["scripts/e2e"] : []),
    "tests/quality",
    "eslint.config.mjs",
  ]);
  for (const result of reports) {
    const file = result.filePath.slice(process.cwd().length + 1).replaceAll("\\", "/");
    const text = sources.get(file) ?? readFileSync(result.filePath, "utf8");
    for (const message of result.messages) {
      if (message.fatal) throw new Error(`${file}:${message.line}: ${message.message}`);
      const evidence = text.split(/\r?\n/)[(message.line ?? 1) - 1]?.trim() ?? "";
      issues.push({
        file,
        rule: message.ruleId ?? "eslint/parse",
        line: message.line ?? 1,
        message: message.message,
        evidence,
      });
    }
  }
}
if (mode === "check" || mode === "architecture") {
  for (const [file, source] of sources) {
    issues.push(...architectureIssues(file, source));
    const limit = lineLimit(file);
    const count = source.trimEnd().split("\n").length;
    if (limit && count > limit) {
      lines[file] = count;
      if (count > Math.max(limit, baseline.lines[file] ?? 0))
        failures.push(
          `${file}: size/lines: ${count} > ${Math.max(limit, baseline.lines[file] ?? 0)}; Split independent responsibilities; existing oversized files must not grow.`,
        );
    }
  }
}
if (mode === "check" || mode === "format") {
  const config = await prettier.resolveConfig("package.json");
  for (const [file, source] of sources) {
    let formatted;
    if (file.endsWith(".rs")) {
      const result = spawnSync(
        "rustfmt",
        ["--edition", "2021", "--emit", "stdout", "--config", "skip_children=true"],
        { input: source, encoding: "utf8", maxBuffer: 8 * 1024 * 1024 },
      );
      if (result.error || result.status !== 0)
        throw new Error(`${file}: rustfmt failed: ${result.error?.message ?? result.stderr}`);
      formatted = result.stdout;
    } else {
      formatted = await prettier.format(source, { ...config, filepath: file });
    }
    // Git may convert LF to CRLF at checkout; compare formatting independently
    // of that platform conversion, without changing hashes for baseline entries.
    if (source.replaceAll("\r\n", "\n") !== formatted.replaceAll("\r\n", "\n")) {
      formatting[file] = hash(source);
      if (baseline.formatting[file] !== formatting[file])
        failures.push(
          `${file}: format: ${file.endsWith(".rs") ? "rustfmt" : "Prettier"} formatting does not match; existing exceptions apply only to unchanged content.`,
        );
    }
  }
}

if (mode === "rust" || writeBaseline) {
  const result = spawnSync(
    "cargo",
    [
      "clippy",
      "--locked",
      "--manifest-path",
      "src-tauri/Cargo.toml",
      "--all-targets",
      "--message-format=json",
    ],
    { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
  );
  if (result.error) throw result.error;
  const seen = new Set();
  for (const line of result.stdout.split("\n").filter(Boolean)) {
    const event = JSON.parse(line);
    if (event.reason !== "compiler-message") continue;
    const diagnostic = event.message;
    if (diagnostic.level === "error") console.error(diagnostic.rendered ?? diagnostic.message);
    if (diagnostic.level !== "warning") continue;
    const span = diagnostic.spans.find((entry) => entry.is_primary);
    if (!span) throw new Error(`Clippy warning has no source span: ${diagnostic.message}`);
    const relative = span.file_name.replaceAll("\\", "/");
    const file =
      relative.startsWith("src/") || relative.startsWith("tests/")
        ? `src-tauri/${relative}`
        : relative;
    const issue = {
      file,
      rule: `rust/${diagnostic.code?.code ?? "warning"}`,
      line: span.line_start,
      message: diagnostic.message,
      evidence: span.text.map((entry) => entry.text.trim()).join("\n"),
    };
    const identity = `${fingerprint(issue)}:${span.byte_start}:${span.byte_end}`;
    if (!seen.has(identity)) {
      issues.push(issue);
      seen.add(identity);
    }
  }
  // Compilation errors, build-script failures and Clippy errors are never debt.
  if (result.status !== 0)
    throw new Error(`Clippy/compile failed (${result.status}): ${result.stderr}`);
}

if (writeBaseline) {
  const accepted = {};
  for (const issue of issues) {
    const key = fingerprint(issue);
    accepted[key] ??= {
      file: issue.file,
      rule: issue.rule,
      line: issue.line,
      message: issue.message,
      evidence: issue.evidence,
      count: 0,
    };
    accepted[key].count++;
  }
  writeFileSync(
    baselinePath,
    JSON.stringify(
      {
        version: 1,
        note: "Initial debt may only decrease; new code and changed formatting must not add exceptions.",
        issues: accepted,
        formatting,
        lines,
      },
      null,
      2,
    ) + "\n",
  );
  console.log(
    `Baseline written: ${issues.length} diagnostics, ${Object.keys(formatting).length} unformatted files, ${Object.keys(lines).length} oversized files. This records debt; it does not fix it.`,
  );
  process.exit(0);
}

if (baseIndex >= 0) {
  const commit = git("rev-parse", "--verify", `${args[baseIndex + 1]}^{commit}`);
  const listed = git("ls-tree", "--name-only", commit, "--", baselinePath);
  if (listed) {
    const previous = JSON.parse(git("show", `${commit}:${baselinePath}`));
    failures.push(...baselineGrowth(baseline, previous));
  } else {
    console.log("Base has no baseline: initial adoption; review the initial debt inventory.");
  }
}
const newIssues = excessIssues(issues, baseline.issues, fingerprint);
for (const issue of newIssues)
  failures.push(`${issue.file}:${issue.line}: ${issue.rule}: ${issue.message}`);
if (failures.length) {
  console.error(failures.join("\n"));
  console.error(
    `FAIL: ${failures.length} new violations. Baseline exceptions must not be expanded.`,
  );
  process.exitCode = 1;
} else {
  console.log(
    `PASS ${mode}: no new violations; existing debt observed: ${issues.length} diagnostics, ${Object.keys(formatting).length} unformatted files, ${Object.keys(lines).length} oversized files.`,
  );
}
