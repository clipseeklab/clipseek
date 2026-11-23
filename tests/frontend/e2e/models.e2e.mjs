import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { browser, $, $$ } from "@wdio/globals";
import {
  invoke,
  click,
  openSettings,
  closeDialog,
  importFiles,
  waitAssets,
  current,
} from "./helpers.mjs";

export function registerModelCases() {
  const fixtures = process.env.CLIPSTUDIO_EMBEDDING_FIXTURES;
  const names = ["photo.png", "short.mp4", "limit.mp4", "too-long.mp4"];
  const files = names.map((name) => path.join(fixtures, name));
  const originals = files.map((file) => readFileSync(file));
  it("enables automatic indexing through model settings while retaining Q8", async () => {
    assert.equal((await invoke("model_status")).ready, true);
    await openSettings();
    await click("Index management");
    const toggle = await $("#auto-visual-index");
    assert.equal(await toggle.getAttribute("aria-checked"), "false");
    await toggle.click();
    await browser.waitUntil(async () => (await invoke("settings_get")).autoVisualIndex);
    assert.equal((await invoke("model_status")).precision, "q8");
    await closeDialog();
  });
  it("imports and indexes real media, accepts exactly 32 seconds, and rejects 33 seconds", async () => {
    const summary = await importFiles(files);
    assert.equal(summary.done, 3);
    assert.equal(summary.failed, 1);
    assert.equal(summary.duplicates, 0);
    const result = await waitAssets(3);
    assert.deepEqual(result.items.map((asset) => asset.fileName).sort(), names.slice(0, 3).sort());
    assert.equal(result.items.find((asset) => asset.fileName === "limit.mp4").durationMs, 32000);
    try {
      await browser.waitUntil(async () => (await invoke("index_summary")).visual === 3, {
        timeout: 90000,
        timeoutMsg: "Real Q8 indexing did not finish",
      });
    } catch (error) {
      const library = await current();
      console.error("Q8 index state:", await invoke("index_summary"));
      console.error(
        "Q8 processing state:",
        await invoke("processing_list", {
          libraryId: library.id,
          filter: {},
        }),
      );
      throw error;
    }
    const status = await invoke("index_summary");
    assert.equal(status.visualModelReady, true);
    assert.equal(status.tasks.error ?? 0, 0);
    for (const asset of result.items) {
      const detail = await invoke("asset_detail", { id: asset.id });
      assert.equal(detail.frames.length, 1);
      assert.equal(detail.frames[0].seq, 0);
      assert.equal(detail.frames[0].timestampMs, 0);
      assert.equal(detail.storage, "reference");
    }
    files.forEach((file, i) => assert.deepEqual(readFileSync(file), originals[i]));
    console.log("Native Q8 E2E backend:", (await invoke("model_status")).execution?.backend);
  });
  it("searches real indexed media through the UI and ranks the red photo first", async () => {
    await $('a[href="#/search"]').click();
    const input = await $('input[placeholder^="Describe"]');
    await input.waitForDisplayed();
    await input.setValue("a solid red image");
    assert.equal(await input.getValue(), "a solid red image");
    await click("Search");
    const photo = await $('img[alt="photo.png"]');
    await photo.waitForDisplayed({ timeout: 90000 });
    const images = await $$('img[alt="photo.png"], img[alt="short.mp4"], img[alt="limit.mp4"]');
    assert.equal(await images[0].getAttribute("alt"), "photo.png");
    await $('a[href="#/assets"]').click();
  });
}
