import assert from "node:assert/strict";
import test from "node:test";
import {
  architectureIssues,
  baselineGrowth,
  excessIssues,
  lineLimit,
  maskRust,
} from "../../scripts/quality/rules.mjs";

const rules = (file, source) => architectureIssues(file, source).map((issue) => issue.rule);

test("IPC boundary rejects aliases, namespace imports, re-exports and dynamic imports", () => {
  for (const source of [
    'import { invoke as run } from "@tauri-apps/api/core";',
    'import * as core from "@tauri-apps/api/core";',
    'export { invoke as run } from "@tauri-apps/api/core";',
    'export * from "@tauri-apps/api/core";',
    'export * as core from "@tauri-apps/api/core";',
    'const core = await import("@tauri-apps/api/core");',
    'const core = require("@tauri-apps/api/core");',
  ])
    assert.deepEqual(
      rules("src/components/Example.tsx", source),
      ["architecture/ipc-boundary"],
      source,
    );
});

test("preview conversion and the designated IPC entry remain allowed", () => {
  assert.deepEqual(
    rules("src/components/Example.tsx", 'import { convertFileSrc } from "@tauri-apps/api/core";'),
    [],
  );
  assert.deepEqual(rules("src/lib/api.ts", 'import { invoke } from "@tauri-apps/api/core";'), []);
  assert.deepEqual(
    rules("src/components/Example.tsx", '// import { invoke } from "@tauri-apps/api/core";'),
    [],
  );
});

test("test bodies and fixtures stay outside business source", () => {
  assert.deepEqual(rules("src/test/fixtures/sample.ts", "export const sample = {};"), [
    "architecture/test-location",
  ]);
  assert.deepEqual(rules("src/example.spec.ts", ""), ["architecture/test-location"]);
  assert.deepEqual(rules("src/example.ts", 'import { test } from "vitest";'), [
    "architecture/test-location",
  ]);
  assert.deepEqual(rules("tests/frontend/example.test.ts", 'import { test } from "vitest";'), []);
  assert.deepEqual(rules("src-tauri/src/db/example.rs", "#[tokio::test]\nasync fn example() {}"), [
    "architecture/test-location",
  ]);
  assert.deepEqual(
    rules(
      "src-tauri/src/db/example.rs",
      '#[cfg(test)]\n#[path = "../../tests/unit/example.rs"]\nmod tests;',
    ),
    [],
  );
});

test("Rust service dependency checks cover direct, grouped and relative paths", () => {
  for (const source of [
    "crate::commands::assets::move_assets();",
    "use crate::{db, commands::assets};",
    "use super::commands;",
  ]) {
    assert.deepEqual(rules("src-tauri/src/services.rs", source), [
      "architecture/command-dependency",
    ]);
  }
  assert.deepEqual(rules("src-tauri/src/commands/assets.rs", "crate::commands::blocking();"), []);
  assert.deepEqual(rules("src-tauri/src/db/example.rs", "use tauri::AppHandle;"), [
    "architecture/dao-runtime",
    "architecture/dao-runtime",
  ]);
});

test("Rust comments, nested comments, raw strings and char literals are not code", () => {
  const source = [
    "// crate::commands::example();",
    "/* nested /* #[test] */ crate::commands */",
    'let sql = r###"#[test] crate::commands tauri::AppHandle"###;',
    'let bytes = br#"crate::commands"#;',
    String.raw`let text = "escaped \" crate::commands";`,
    "let quote = '\\'';",
    "fn borrow<'a>(v: &'a str) {}",
  ].join("\n");
  assert.deepEqual(rules("src-tauri/src/db/example.rs", source), []);
  assert.equal(maskRust(source).split("\n").length, source.split("\n").length);
  assert.match(maskRust(source), /borrow<'a>/);
});

test("command adapters cannot restore public service compatibility exports", () => {
  for (const source of [
    "pub use crate::services::assets::{trash_assets, restore_assets};",
    "pub(crate) use crate::services::sessions::current as session_arc;",
    "pub use crate::{services::search::SearchHit};",
  ]) {
    assert.deepEqual(rules("src-tauri/src/commands/assets.rs", source), [
      "architecture/service-reexport",
    ]);
  }
  assert.deepEqual(
    rules("src-tauri/src/commands/search.rs", "use crate::services::search::SearchHit;"),
    [],
  );
  assert.deepEqual(
    rules("src-tauri/src/commands/search.rs", "// pub use crate::services::search::*;"),
    [],
  );
});

test("a baseline accepts only the recorded number of identical violations", () => {
  const issue = { id: "existing" };
  const key = (item) => item.id;
  const accepted = { existing: { count: 1 } };
  assert.deepEqual(excessIssues([issue], accepted, key), []);
  assert.deepEqual(excessIssues([issue, issue], accepted, key), [issue]);
  assert.deepEqual(excessIssues([{ id: "new" }], accepted, key), [{ id: "new" }]);
});

test("CI baseline comparison allows cleanup but rejects debt growth or content changes", () => {
  const previous = {
    issues: { old: { file: "old.ts", rule: "lint", count: 2 } },
    formatting: { "old.ts": "original-hash" },
    lines: { "old.ts": 600 },
  };
  assert.deepEqual(baselineGrowth({ issues: {}, formatting: {}, lines: {} }, previous), []);
  assert.deepEqual(baselineGrowth(previous, previous), []);
  const changed = structuredClone(previous);
  changed.issues.old.count++;
  changed.issues.new = { file: "new.ts", rule: "lint", count: 1 };
  changed.formatting["old.ts"] = "updated-hash";
  changed.formatting["new.ts"] = "new-hash";
  changed.lines["old.ts"]++;
  changed.lines["new.rs"] = 800;
  assert.equal(baselineGrowth(changed, previous).length, 6);
});

test("file size budgets apply to business source, with explicit data/test exclusions", () => {
  assert.equal(lineLimit("src/components/Example.tsx"), 500);
  assert.equal(lineLimit("src-tauri/src/services.rs"), 700);
  assert.equal(lineLimit("src/i18n/zh.ts"), null);
  assert.equal(lineLimit("src/vite-env.d.ts"), null);
  assert.equal(lineLimit("tests/frontend/example.test.ts"), null);
});

test("new Rust lint suppressions and inline fixture modules cannot hide debt", () => {
  assert.deepEqual(rules("src-tauri/src/example.rs", "#![allow(warnings)]"), [
    "architecture/lint-suppression",
  ]);
  assert.deepEqual(
    rules("src-tauri/src/example.rs", "#[expect(clippy::unwrap_used)] fn run() {}"),
    ["architecture/lint-suppression"],
  );
  assert.deepEqual(
    rules("src-tauri/src/example.rs", "#[cfg(test)] mod tests { fn fixture() {} }"),
    ["architecture/test-location"],
  );
});

test("migrated query identities cannot be declared again in business modules", () => {
  for (const source of [
    'useQuery({queryKey: ["processing", libraryId, "summary"]});',
    'client.invalidateQueries({queryKey: ["indexTasks"]});',
    'useQuery({queryKey: ["indexSummary", libraryId]});',
    'useQuery({queryKey: ["target-options", libraryId]});',
    'client.getQueryData(["asset-video-codec", libraryId, id]);',
    'useQuery({queryKey: ["import-notices", libraryId]});',
    'useQuery({queryKey: ["folders", libraryId]});',
    'client.invalidateQueries({queryKey: ["tags"]});',
    'client.setQueryData(["assetDetail", id], next);',
    'client.setQueryData(["asset-detail", libraryId, id], next);',
    'useQuery({queryKey: ["subtitle-info", id] as const});',
  ])
    assert.deepEqual(rules("src/components/Example.tsx", source), [
      "architecture/query-key-factory",
    ]);
  for (const source of [
    "useQuery({queryKey: queryKeys.folders.sidebar(libraryId)});",
    "client.invalidateQueries();",
    'const text = "queryKey: [\\"tags\\"]";',
    'useQuery({queryKey: ["unmigrated-domain", id]});',
  ])
    assert.deepEqual(rules("src/components/Example.tsx", source), []);
});
