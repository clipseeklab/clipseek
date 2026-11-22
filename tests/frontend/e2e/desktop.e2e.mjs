import assert from "node:assert/strict";
import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { browser, $ } from "@wdio/globals";
import {
  invoke,
  click,
  readyShell,
  openSettings,
  closeDialog,
  createLibrary,
  importFiles,
  waitAssets,
  assets,
  current,
  switchLibrary,
  trashImage,
  openDropdown,
} from "./helpers.mjs";

import { registerModelCases } from "./models.e2e.mjs";

const root = process.env.CLIPSEEK_E2E_ROOT;
const source = path.join(root, "media/synthetic 你好.png");
const original = readFileSync(source);
describe("Native desktop E2E", () => {
  before(async () => {
    await browser.execute(() => {
      const style = document.createElement("style");
      style.textContent =
        "*, *::before, *::after { animation: none !important; transition: none !important; scroll-behavior: auto !important; }";
      document.head.append(style);
    });
  });
  it("starts in English with Q8 and reference-only defaults", async () => {
    await $("body").waitForDisplayed();
    const settings = await invoke("settings_get");
    assert.equal(settings.language, "en");
    assert.equal(settings.importMode, "reference");
    assert.equal(settings.embeddingPrecision, "q8");
    assert.equal(settings.mcpEnabled, false);
    assert.equal(await browser.getTitle(), "ClipSeek Open");
  });
  it("restores the fixture library through native startup and renders it", async () => {
    await readyShell();
    assert.equal((await current()).name, "E2E Primary");
    assert.match(await $("aside").getText(), /E2E Primary/);
  });
  it("opens and dismisses first-use tips before continuing the workspace journey", async () => {
    const trigger = await $('button[aria-label="Usage tips"]');
    if ((await trigger.getAttribute("aria-expanded")) !== "true") await trigger.click();
    await $('button[aria-label="Dismiss"]').waitForDisplayed();
    await browser.waitUntil(
      async () =>
        browser.execute(() => {
          const tips = JSON.parse(localStorage.getItem("clipstudio-open:usage-tips") ?? "null");
          return tips?.presented === true && tips.pending === false;
        }),
      { timeoutMsg: "First-use tips did not acknowledge presentation" },
    );
    await $('button[aria-label="Dismiss"]').click();
    await $('button[aria-label="Dismiss"]').waitForDisplayed({ reverse: true });
  });
  it("creates a Unicode-named library through the UI and native directory picker boundary", async () => {
    const secondary = await createLibrary("E2E 中文");
    assert(secondary.path.startsWith(path.join(root, "libraries") + path.sep));
    assert.equal((await invoke("library_list")).length, 2);
  });
  it("imports by reference through the real queue and displays an actual thumbnail", async () => {
    await importFiles([source]);
    const result = await waitAssets(1);
    const asset = result.items[0];
    assert.equal(asset.storage, "reference");
    assert.equal(asset.sourcePath, source);
    assert.equal(asset.absPath, source);
    const image = await $('img[alt="synthetic 你好.png"]');
    await image.waitForDisplayed();
    await browser.waitUntil(async () => (await image.getProperty("naturalWidth")) > 0, {
      timeoutMsg: "Actual asset thumbnail did not decode in WKWebView",
    });
    assert.deepEqual(readFileSync(source), original);
  });
  it("deduplicates repeated imports without creating a second asset", async () => {
    const result = await importFiles([source]);
    assert.equal(result.duplicates, 1);
    assert.equal(result.failed, 0);
    assert.equal((await assets()).total, 1);
    assert.deepEqual(readFileSync(source), original);
  });
  it("isolates asset lists when switching libraries and rejects unknown library headers", async () => {
    const secondary = await current();
    await switchLibrary("E2E Primary");
    assert.equal((await assets()).total, 0);
    await $('img[alt="synthetic 你好.png"]').waitForDisplayed({ reverse: true });
    assert.equal((await assets(secondary.id)).total, 1);
    await assert.rejects(assets("not-a-registered-library"));
    await switchLibrary("E2E 中文");
    await $('img[alt="synthetic 你好.png"]').waitForDisplayed();
  });
  it("opens compact model management with Q8 selected and no path editor", async () => {
    await openSettings();
    await click("Model management");
    await $("#settings-model").waitForDisplayed();
    assert.equal(await $('#settings-model button[aria-pressed="true"]').getText(), "Q8");
    assert.equal(await $("#settings-model input").isExisting(), false);
    assert.doesNotMatch(await $('[role="dialog"]').getText(), /Copy into library|Import mode/);
    await closeDialog();
  });
  it("starts a real read-only loopback MCP listener, rejects unauthenticated access, and stops it", async () => {
    await openSettings();
    await click("MCP interface");
    const toggle = await $('[role="switch"][aria-label="Enable MCP service"]');
    assert.equal(await toggle.getAttribute("aria-checked"), "false");
    await toggle.click();
    await browser.waitUntil(async () => (await invoke("mcp_status")).running);
    const status = await invoke("mcp_status");
    assert.equal(status.writeEnabled, false);
    assert.equal(status.listenMode, "loopback");
    assert.equal((await globalThis.fetch(status.url)).status, 401);
    await toggle.click();
    await browser.waitUntil(async () => !(await invoke("mcp_status")).running);
    await closeDialog();
  });
  it("restores and permanently removes library references while preserving the original", async () => {
    await trashImage("synthetic 你好.png");
    await waitAssets(0);
    await $('a[href="#/trash"]').click();
    await click("Restore All");
    await browser.waitUntil(async () => (await assets()).total === 1);
    await $('a[href="#/assets"]').click();
    await $('img[alt="synthetic 你好.png"]').waitForDisplayed();
    await trashImage("synthetic 你好.png");
    await waitAssets(0);
    await $('a[href="#/trash"]').click();
    await click("Empty Trash");
    await $('[role="alertdialog"]').waitForDisplayed();
    await click("Delete");
    await browser.waitUntil(async () => (await assets(undefined, { inTrash: true })).total === 0);
    assert.deepEqual(readFileSync(source), original);
    await $('a[href="#/assets"]').click();
  });
  if (process.env.CLIPSEEK_E2E_PROFILE === "models") registerModelCases();
  it("persists UI language and records state for a separate application-process restart", async () => {
    await openSettings();
    await click("General");
    await openDropdown('#settings-general [role="combobox"]');
    await $('//*[@role="option" and normalize-space(.)="中文"]').click();
    await browser.waitUntil(async () => (await invoke("settings_get")).language === "zh");
    await closeDialog();
    assert.match(await $("aside").getText(), /设置/);
    const library = await current();
    writeFileSync(
      path.join(root, "expected.json"),
      JSON.stringify({
        libraryId: library.id,
        name: library.name,
        assets: (await assets()).total,
        visual: (await invoke("index_summary")).visual,
        process: JSON.parse(readFileSync(path.join(root, "native-process.json"), "utf8")),
      }),
    );
  });
});
