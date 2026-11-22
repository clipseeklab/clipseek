import assert from "node:assert/strict";
import { mkdtemp, writeFile, rm, readFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { createServer } from "node:net";
import test from "node:test";
import { assertAutomationBinary, freePort } from "../../scripts/e2e/safety.mjs";

test("native launcher rejects ordinary or incorrectly scoped binaries before startup", async (t) => {
  const root = await mkdtemp(path.join(os.tmpdir(), "clipseek-e2e-guard-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const file = path.join(root, "synthetic-binary");
  for (const content of [
    "ordinary executable",
    "E2E builds require CLIPSEEK_E2E_ROOT",
    "org.clipstudio.open.e2e",
  ]) {
    await writeFile(file, content);
    assert.throws(() => assertAutomationBinary(file), /Refusing to launch/);
  }
  await writeFile(file, "E2E builds require CLIPSEEK_E2E_ROOT\norg.clipstudio.open.e2e");
  assert.doesNotThrow(() => assertAutomationBinary(file));
});
test("loopback port allocation closes its reservation before native startup", async () => {
  const port = await freePort();
  assert(port >= 1024 && port <= 65535);
  const server = createServer();
  try {
    await new Promise((resolve, reject) => {
      server.once("error", reject);
      server.listen(port, "127.0.0.1", resolve);
    });
    assert.equal(server.address().port, port);
  } finally {
    await new Promise((resolve, reject) =>
      server.close((error) => (error ? reject(error) : resolve())),
    );
  }
});
test("automation uses a separate application identifier and never extends production capabilities", async () => {
  const json = async (file) => JSON.parse(await readFile(file, "utf8"));
  const production = await json("src-tauri/tauri.conf.json");
  const automation = await json("tests/frontend/e2e/tauri.conf.json");
  const capabilities = await json("src-tauri/capabilities/default.json");
  assert.notEqual(automation.identifier, production.identifier);
  assert.equal(automation.identifier, "org.clipstudio.open.e2e");
  assert(!capabilities.permissions.some((permission) => /wdio/.test(permission)));
  assert(automation.app.security.capabilities[0].permissions.includes("wdio:default"));
  const cargo = await readFile("src-tauri/Cargo.toml", "utf8");
  assert.match(cargo, /default = \[\]/);
  assert.match(cargo, /tauri-plugin-wdio = \{[^\n]*optional = true/);
  assert.match(cargo, /tauri-plugin-wdio-webdriver = \{[^\n]*optional = true/);
  const entry = await readFile("src-tauri/src/lib.rs", "utf8");
  assert.match(entry, /cfg\(all\(feature = "e2e", not\(debug_assertions\)\)\)/);
  assert.match(entry, /compile_error!\("The e2e automation feature must never/);
});
