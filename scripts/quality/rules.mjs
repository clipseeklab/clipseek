import ts from "typescript";

// Keep line positions while masking comments and literals, including nested Rust
// comments and raw strings. Patterns below inspect code, not examples in strings.
export function maskRust(source) {
  let result = "";
  let offset = 0;
  const mask = (text) => text.replace(/[^\n]/g, " ");
  while (offset < source.length) {
    const rest = source.slice(offset);
    let length;
    if (rest.startsWith("//")) {
      length = rest.indexOf("\n");
      if (length < 0) length = rest.length;
    } else if (rest.startsWith("/*")) {
      let depth = 1;
      length = 2;
      while (length < rest.length && depth) {
        if (rest.startsWith("/*", length)) {
          depth++;
          length += 2;
        } else if (rest.startsWith("*/", length)) {
          depth--;
          length += 2;
        } else length++;
      }
    } else {
      const raw = /^(?:br|cr|r)(#*)"/.exec(rest);
      if (raw) {
        const close = rest.indexOf(`"${raw[1]}`, raw[0].length);
        length = close < 0 ? rest.length : close + raw[1].length + 1;
      } else if (rest[0] === '"') {
        length = 1;
        while (length < rest.length) {
          if (rest[length] === "\\") length += 2;
          else if (rest[length++] === '"') break;
        }
      } else {
        // A lifetime such as 'a is code, not a character literal.
        length =
          /^'(?:\\(?:u\{[0-9a-fA-F]+\}|x[0-9a-fA-F]{2}|.)|[^'\\\n])'/u.exec(rest)?.[0].length ?? 0;
      }
    }
    if (length) {
      result += mask(rest.slice(0, length));
      offset += length;
    } else result += source[offset++];
  }
  return result;
}

export function architectureIssues(file, source) {
  const issues = [];
  const report = (rule, offset, message) =>
    issues.push({
      file,
      rule,
      line: source.slice(0, offset).split("\n").length,
      message,
      evidence: source
        .slice(
          offset,
          source.indexOf("\n", offset) < 0 ? source.length : source.indexOf("\n", offset),
        )
        .trim(),
    });
  if (file.startsWith("src/") && /\.[cm]?tsx?$/.test(file)) {
    if (/\/(?:__tests__|test|fixtures)\/|\.(?:test|spec)\./.test(file)) {
      report(
        "architecture/test-location",
        0,
        "Business source must not contain tests or fixtures; use tests/frontend.",
      );
    }
    const ast = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
    const migratedQueryRoots = new Set([
      "assets",
      "folders",
      "tags",
      "assetDetail",
      "asset-detail",
      "subtitle-info",
      "subtitle-cues",
      "asset-storage-stats",
      "processing",
      "indexTasks",
      "indexSummary",
      "target-options",
      "asset-video-codec",
      "import-notices",
    ]);
    const hasMigratedLiteralKey = (expression) => {
      while (
        expression &&
        (ts.isAsExpression(expression) ||
          ts.isSatisfiesExpression(expression) ||
          ts.isParenthesizedExpression(expression))
      ) {
        expression = expression.expression;
      }
      return (
        expression &&
        ts.isArrayLiteralExpression(expression) &&
        expression.elements.length > 0 &&
        ts.isStringLiteral(expression.elements[0]) &&
        migratedQueryRoots.has(expression.elements[0].text)
      );
    };
    const visit = (node) => {
      const inlineQueryKey =
        ts.isPropertyAssignment(node) &&
        (ts.isIdentifier(node.name) || ts.isStringLiteral(node.name)) &&
        node.name.text === "queryKey" &&
        hasMigratedLiteralKey(node.initializer);
      const inlineCacheKey =
        ts.isCallExpression(node) &&
        ts.isPropertyAccessExpression(node.expression) &&
        ["setQueryData", "getQueryData", "getQueryState"].includes(node.expression.name.text) &&
        hasMigratedLiteralKey(node.arguments[0]);
      if (inlineQueryKey || inlineCacheKey) {
        report(
          "architecture/query-key-factory",
          node.getStart(ast),
          "Use src/lib/queryKeys.ts for migrated cache keys so queries, writes, and invalidation share one identity.",
        );
      }
      if (
        (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) &&
        node.moduleSpecifier &&
        ts.isStringLiteral(node.moduleSpecifier)
      ) {
        const module = node.moduleSpecifier.text;
        if (module === "vitest" || module.startsWith("@testing-library/")) {
          report(
            "architecture/test-location",
            node.getStart(ast),
            "Only test directories may import test frameworks.",
          );
        }
        if (module === "@tauri-apps/api/core" && file !== "src/lib/api.ts") {
          const bindings = ts.isImportDeclaration(node)
            ? node.importClause?.namedBindings
            : node.exportClause;
          const exposesInvoke =
            !bindings ||
            ts.isNamespaceImport(bindings) ||
            ts.isNamespaceExport(bindings) ||
            bindings.elements?.some((item) => (item.propertyName ?? item.name).text === "invoke");
          if (exposesInvoke)
            report(
              "architecture/ipc-boundary",
              node.getStart(ast),
              "Import or export invoke only in src/lib/api.ts; other modules use api.",
            );
        }
      }
      if (
        ts.isCallExpression(node) &&
        node.arguments.some(
          (arg) => ts.isStringLiteral(arg) && arg.text === "@tauri-apps/api/core",
        ) &&
        (node.expression.kind === ts.SyntaxKind.ImportKeyword ||
          node.expression.getText(ast) === "require") &&
        file !== "src/lib/api.ts"
      ) {
        report(
          "architecture/ipc-boundary",
          node.getStart(ast),
          "Do not dynamically load core to bypass the shared IPC entry point.",
        );
      }
      ts.forEachChild(node, visit);
    };
    visit(ast);
  }
  if (file.startsWith("src-tauri/src/") && file.endsWith(".rs")) {
    const code = maskRust(source);
    const patterns = [
      [
        "architecture/lint-suppression",
        /#\s*!?\[\s*(?:allow|expect)\s*\(/g,
        "Do not add Rust lint suppressions; fix the issue or review a shared rule change.",
      ],
      [
        "architecture/test-location",
        /#\s*\[\s*(?:(?:tokio|tauri)::)?test\s*\]/g,
        "Keep test bodies in src-tauri/tests; external test module declarations are allowed.",
      ],
      [
        "architecture/test-location",
        /#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\](?:\s*#\[[^\]]*\])*\s*(?:pub\s+)?mod\s+\w+\s*\{/g,
        "Business source may declare external test modules, but must not contain inline tests or fixtures.",
      ],
    ];
    if (file.startsWith("src-tauri/src/commands/")) {
      patterns.push([
        "architecture/service-reexport",
        /\bpub\s*(?:\([^)]*\)\s*)?use\s+crate\s*::\s*(?:services\b|\{[^;]*?\bservices\b)/g,
        "Commands must not re-export services; callers depend on services directly.",
      ]);
    }
    if (!file.startsWith("src-tauri/src/commands/") && file !== "src-tauri/src/lib.rs") {
      patterns.push([
        "architecture/command-dependency",
        /\b(?:crate|super)\s*::\s*commands\b|\buse\s+crate\s*::\s*\{[^;]*?\bcommands\b/g,
        "Business modules must not depend on commands; put shared behavior in services.",
      ]);
    }
    if (file.startsWith("src-tauri/src/db/")) {
      patterns.push([
        "architecture/dao-runtime",
        /\btauri\s*::|\bAppHandle\b/g,
        "DAOs must not depend on the Tauri runtime.",
      ]);
    }
    for (const [rule, pattern, message] of patterns) {
      for (const match of code.matchAll(pattern)) report(rule, match.index, message.trim());
    }
  }
  return issues;
}

export function lineLimit(file) {
  if (/^src\/i18n\//.test(file) || file.endsWith(".d.ts")) return null;
  if (/^src\/.*\.tsx?$/.test(file)) return 500;
  if (/^src-tauri\/src\/.*\.rs$/.test(file)) return 700;
  return null;
}

export function excessIssues(issues, accepted, fingerprint) {
  const seen = new Map();
  return issues.filter((issue) => {
    const key = fingerprint(issue);
    const count = (seen.get(key) ?? 0) + 1;
    seen.set(key, count);
    return count > (accepted[key]?.count ?? 0);
  });
}

export function baselineGrowth(current, previous) {
  const failures = [];
  for (const [key, value] of Object.entries(current.issues)) {
    if (value.count > (previous.issues[key]?.count ?? 0))
      failures.push(`baseline: Cannot add diagnostic exceptions ${value.file} ${value.rule}`);
  }
  for (const [file, value] of Object.entries(current.formatting)) {
    if (previous.formatting[file] !== value)
      failures.push(`baseline: Cannot add or replace formatting exceptions ${file}`);
  }
  for (const [file, value] of Object.entries(current.lines)) {
    if (value > (previous.lines[file] ?? 0))
      failures.push(`baseline: Cannot increase line-count exceptions ${file}`);
  }
  return failures;
}
