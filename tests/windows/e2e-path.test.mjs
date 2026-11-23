import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, realpathSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { canonicalSandboxPath } from "../../scripts/e2e/platform.mjs";

test("Windows short-path aliases resolve to the original full sandbox directory", (t) => {
  assert.equal(process.platform, "win32", "Run this regression test on native Windows");
  const parent = realpathSync.native(os.tmpdir());
  const root = mkdtempSync(path.join(parent, "ClipSeek sandbox path "));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const script = `(New-Object -ComObject Scripting.FileSystemObject).GetFolder('${root.replaceAll("'", "''")}').ShortPath`;
  const result = spawnSync(
    "powershell.exe",
    [
      "-NoProfile",
      "-NonInteractive",
      "-EncodedCommand",
      Buffer.from(script, "utf16le").toString("base64"),
    ],
    { encoding: "utf8" },
  );
  assert.equal(result.status, 0, result.stderr);
  const short = result.stdout.trim();
  assert(short.length > 0, "Windows must return a directory path");
  const expected = path.toNamespacedPath(root);
  assert.equal(canonicalSandboxPath(short), expected);
  assert.equal(canonicalSandboxPath(root), expected);
});
