import assert from "node:assert/strict";
import path from "node:path";
import { browser } from "@wdio/globals";
import { invoke } from "./helpers.mjs";

describe("Isolated native fixture bootstrap", () => {
  before(async () => {
    await browser.execute(() => {
      localStorage.clear();
      sessionStorage.clear();
    });
  });
  it("loads real default settings from an empty test profile", async () => {
    const settings = await invoke("settings_get");
    assert.equal(settings.language, "en");
    assert.equal(settings.embeddingPrecision, "q8");
    assert.equal(settings.importMode, "reference");
    assert.equal(settings.mcpEnabled, false);
    assert.deepEqual(await invoke("library_list"), []);
    assert.equal(await browser.getTitle(), "ClipSeek Open");
  });
  it("seeds a real fixture library and persists completed onboarding before app restart", async () => {
    const library = await invoke("library_create", {
      name: "E2E Primary",
      path: path.join(process.env.CLIPSEEK_E2E_ROOT, "libraries/primary"),
    });
    assert.equal(library.name, "E2E Primary");
    await invoke("onboarding_set", { currentStep: 3, completed: true });
    assert.equal((await invoke("onboarding_get")).completed, true);
  });
});
