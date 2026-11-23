import assert from "node:assert/strict";
import path from "node:path";
import { browser, $ } from "@wdio/globals";

// Use the same native entry point as core.invoke; WDIO's global wrapper drops headers.
export async function invoke(command, args = {}, options = {}) {
  return browser.tauri.execute(
    (_tauri, cmd, payload, options) => globalThis.__TAURI_INTERNALS__.invoke(cmd, payload, options),
    command,
    args,
    options,
  );
}
export async function button(text) {
  const element = await $(`//button[normalize-space(.)=${JSON.stringify(text)}]`);
  await element.waitForDisplayed();
  return element;
}
export async function click(text) {
  await (await button(text)).click();
}
export async function dialogResult(value) {
  const mock = await browser.tauri.mock("plugin:dialog|open");
  await mock.mockResolvedValueOnce(value);
  return mock;
}
export async function current() {
  return invoke("library_current");
}
export async function assets(libraryId, extra = {}) {
  return invoke(
    "asset_list",
    { filter: { page: 1, pageSize: 100, ...extra } },
    { headers: libraryId ? { "x-clipseek-library": libraryId } : {} },
  );
}
export async function readyShell() {
  await $('a[href="#/assets"]').waitForDisplayed();
  assert.equal(await browser.getTitle(), "ClipSeek Open");
}
export async function openSettings() {
  await click("Settings");
  await $('[role="dialog"]').waitForDisplayed();
}
export async function closeDialog() {
  await browser.keys("Escape");
  await $('[role="dialog"]').waitForDisplayed({ reverse: true });
}

export async function menu(text) {
  const item = await $(`//*[@role="menuitem" and normalize-space(.)=${JSON.stringify(text)}]`);
  await item.waitForDisplayed();
  await item.click();
}
export async function openDropdown(selector) {
  const trigger = await $(selector);
  await trigger.click();
  if ((await trigger.getAttribute("aria-expanded")) !== "true") await browser.keys("ArrowDown");
}
export async function importFiles(paths) {
  const before = await browser.tauri.execute(async () => {
    if (!globalThis.__clipseekE2EImports) {
      globalThis.__clipseekE2EImports = [];
      await globalThis.__TAURI__.event.listen("import://done", (event) =>
        globalThis.__clipseekE2EImports.push(event.payload),
      );
    }
    return globalThis.__clipseekE2EImports.length;
  });
  await dialogResult(paths);
  await openDropdown('button[title="Import"]');
  await menu("Import Assets");
  await browser.waitUntil(
    async () =>
      browser.execute((before) => globalThis.__clipseekE2EImports.length > before, before),
    {
      timeout: process.env.CLIPSEEK_E2E_PROFILE === "models" ? 90000 : 10000,
      timeoutMsg: "Real import completion event did not arrive",
    },
  );
  return browser.execute(() => globalThis.__clipseekE2EImports.at(-1));
}
export async function createLibrary(name) {
  await openDropdown('button[title="Import"]');
  await menu("New Library");
  const input = await $('[role="dialog"] input[placeholder="e.g. My Design Assets"]');
  await input.waitForDisplayed();
  await input.setValue(name);
  const parent = path.join(process.env.CLIPSEEK_E2E_ROOT, "libraries");
  await dialogResult(parent);
  await click("Choose Directory");
  await browser.waitUntil(
    async () => (await $('[role="dialog"] input[readonly]').getValue()) === parent,
    { timeoutMsg: "Directory picker did not resolve to the isolated sandbox" },
  );
  assert.equal(await $('[role="dialog"] input[readonly]').getValue(), parent);
  await click("Create Library");
  await $('[role="dialog"]').waitForDisplayed({ reverse: true });
  await browser.waitUntil(async () => (await current())?.name === name, {
    timeoutMsg: "Created library did not become active",
  });
  return current();
}
export async function switchLibrary(name) {
  await openDropdown(".sidebar-library-row button");
  await menu(name);
  await browser.waitUntil(async () => (await current())?.name === name);
}
export async function waitAssets(count, extra = {}) {
  await browser.waitUntil(
    async () => {
      const result = await assets(undefined, extra);
      return result.total === count && result.items.every((asset) => asset.importStage === "done");
    },
    { timeoutMsg: `Expected ${count} completed assets` },
  );
  return assets(undefined, extra);
}
export async function trashImage(name) {
  await $(`img[alt=${JSON.stringify(name)}]`).click();
  await $('button[aria-label="Move to Trash"]').waitForDisplayed();
  // Metadata reads hold an activity guard; wait for the normal details workflow to finish.
  await $('//h5[normalize-space(.)="Photo EXIF"]').waitForDisplayed();
  await $('button[aria-label="Move to Trash"]').click();
}
