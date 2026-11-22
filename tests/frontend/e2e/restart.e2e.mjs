import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { browser, $ } from "@wdio/globals";
import { invoke, readyShell } from "./helpers.mjs";

const root = process.env.CLIPSEEK_E2E_ROOT;
describe("Native application restart", () => {
  it("restores the active library and language in a new Rust process", async () => {
    const expected = JSON.parse(readFileSync(path.join(root, "expected.json"), "utf8"));
    await readyShell();
    const identity = JSON.parse(readFileSync(path.join(root, "native-process.json"), "utf8"));
    assert.notEqual(identity.pid, expected.process.pid);
    assert.notEqual(identity.boot, expected.process.boot);
    const library = await invoke("library_current");
    assert.equal(library.id, expected.libraryId);
    assert.equal(library.name, expected.name);
    assert.equal((await invoke("settings_get")).language, "zh");
    assert.match(await $("aside").getText(), /设置/);
    assert.equal((await invoke("mcp_status")).running, false);
    assert.equal((await invoke("asset_list", { filter: {} })).total, expected.assets);
    assert.equal((await invoke("index_summary")).visual, expected.visual);
    assert.equal(await browser.getTitle(), "ClipSeek Open");
  });
});
