import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { cpSync, mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const root = fileURLToPath(new URL("../../", import.meta.url));
function fixture(t) {
  const dir = mkdtempSync(join(tmpdir(), "clipseek-quality-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  mkdirSync(join(dir, "scripts"));
  mkdirSync(join(dir, "src"));
  cpSync(join(root, "scripts/quality"), join(dir, "scripts/quality"), { recursive: true });
  cpSync(join(root, ".prettierrc.json"), join(dir, ".prettierrc.json"));
  symlinkSync(join(root, "node_modules"), join(dir, "node_modules"), "junction");
  writeFileSync(join(dir, ".gitignore"), "node_modules/\n");
  const git = (...args) => {
    const result = spawnSync("git", args, { cwd: dir, encoding: "utf8" });
    assert.equal(result.status, 0, result.stderr);
    return result.stdout.trim();
  };
  git("init", "--quiet");
  const baseline = { version: 1, issues: {}, formatting: {}, lines: {} };
  const save = () => writeFileSync(join(dir, ".quality-baseline.json"), JSON.stringify(baseline));
  save();
  const run = (...args) =>
    spawnSync(process.execPath, ["scripts/quality/check.mjs", ...args], {
      cwd: dir,
      encoding: "utf8",
      env: { ...process.env, CI: "true" },
    });
  return { dir, git, baseline, save, run };
}

test("CLI fails new boundary violations and file growth with actionable diagnostics", (t) => {
  const { dir, run } = fixture(t);
  writeFileSync(join(dir, "src/example.ts"), "export const value = 1;\n");
  assert.equal(run("architecture").status, 0);
  writeFileSync(
    join(dir, "src/example.ts"),
    'import { invoke as call } from "@tauri-apps/api/core";\n' + "// another line\n".repeat(500),
  );
  const failed = run("architecture");
  assert.equal(failed.status, 1);
  assert.match(failed.stderr, /architecture\/ipc-boundary/);
  assert.match(failed.stderr, /size\/lines/);
});

test("CLI formatting exception expires when content changes and accepts a real fix", (t) => {
  const { dir, baseline, save, run } = fixture(t);
  const original = "export const value=1\n";
  writeFileSync(join(dir, "src/example.ts"), original);
  baseline.formatting["src/example.ts"] = createHash("sha256").update(original).digest("hex");
  save();
  let result = run("format");
  assert.equal(result.status, 0, result.stderr);
  writeFileSync(join(dir, "src/example.ts"), "export const value=2\n");
  result = run("format");
  assert.equal(result.status, 1);
  assert.match(result.stderr, /src\/example.ts: format/);
  writeFileSync(join(dir, "src/example.ts"), "export const value = 2;\n");
  result = run("format");
  assert.equal(result.status, 0, result.stderr);
});

test("CLI formatting accepts Windows checkout line endings without accepting bad spacing", (t) => {
  const { dir, run } = fixture(t);
  writeFileSync(join(dir, "src/example.ts"), "export const value = 1;\r\n");
  const formatted = run("format");
  assert.equal(formatted.status, 0, formatted.stderr);
  writeFileSync(join(dir, "src/example.ts"), "export const value=1\r\n");
  const unformatted = run("format");
  assert.equal(unformatted.status, 1);
  assert.match(unformatted.stderr, /src\/example.ts: format/);
});

test("CLI compares baseline with the actual base commit and rejects expanding it", (t) => {
  const { git, baseline, save, run } = fixture(t);
  git("add", ".");
  git(
    "-c",
    "user.name=Quality Test",
    "-c",
    "user.email=quality@example.invalid",
    "-c",
    "core.hooksPath=/dev/null",
    "commit",
    "--quiet",
    "-m",
    "fixture baseline",
  );
  const commit = git("rev-parse", "HEAD");
  baseline.formatting["src/unreviewed.ts"] = "new-exception";
  save();
  const result = run("architecture", "--base", commit);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /baseline:.*src\/unreviewed.ts/);
});
