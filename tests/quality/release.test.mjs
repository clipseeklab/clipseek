import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { parse } from "yaml";
import { buildPlan, projectVersion, targets } from "../../scripts/ci/build-plan.mjs";
import { collectInstaller } from "../../scripts/ci/collect-installers.mjs";
import { publishRelease, verifyAssets } from "../../scripts/ci/publish-release.mjs";

async function fixture(t, version = "0.1.0") {
  const root = await mkdtemp(path.join(os.tmpdir(), "clipstudio-release-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  await mkdir(path.join(root, "src-tauri"));
  await writeFile(path.join(root, "package.json"), JSON.stringify({ version }));
  await writeFile(
    path.join(root, "package-lock.json"),
    JSON.stringify({ version, packages: { "": { version } } }),
  );
  await writeFile(path.join(root, "src-tauri/tauri.conf.json"), JSON.stringify({ version }));
  await writeFile(
    path.join(root, "src-tauri/Cargo.toml"),
    `[package]\nname = "clipstudio-open"\nversion = "${version}"\n\n[dependencies]\nexample = "9.0.0"\n`,
  );
  await writeFile(
    path.join(root, "src-tauri/Cargo.lock"),
    `[[package]]\nname = "dependency"\nversion = "9.0.0"\n\n[[package]]\nname = "clipstudio-open"\nversion = "${version}"\n`,
  );
  await writeFile(
    path.join(root, "CHANGELOG.md"),
    `# Changelog\n\n## ${version}\n\n- Example change.\n`,
  );
  return root;
}
async function installers(root) {
  for (const target of targets) {
    const windows = target.platform === "win32";
    const folder = path.join(
      root,
      "src-tauri/target",
      target.target,
      "release/bundle",
      windows ? "nsis" : "dmg",
    );
    await mkdir(folder, { recursive: true });
    await writeFile(
      path.join(folder, windows ? "setup.exe" : "example.dmg"),
      `Synthetic installer for ${target.id}`,
    );
    await collectInstaller(root, target.id);
  }
}
const commit = "a".repeat(40);
const releaseEnv = {
  GITHUB_EVENT_NAME: "push",
  GITHUB_REF_TYPE: "tag",
  GITHUB_REF_NAME: "v0.1.0",
  GITHUB_REPOSITORY: "example/clipstudio",
  GITHUB_SHA: commit,
};
function github(calls, state = "missing", sha = commit) {
  return (args) => {
    calls.push(args);
    if (args[0] === "api") return sha;
    if (args[1] === "view") {
      if (state === "missing")
        throw Object.assign(new Error("not found"), { stderr: "release not found" });
      return JSON.stringify({ isDraft: state === "draft" });
    }
    return "";
  };
}

test("manual builds select platforms and cannot publish, including manual runs on a tag", async (t) => {
  const root = await fixture(t);
  assert.equal(projectVersion(root), "0.1.0");
  const plan = buildPlan(root, { GITHUB_EVENT_NAME: "workflow_dispatch", INPUT_PLATFORM: "macos" });
  assert.deepEqual(plan.matrix.include.map((item) => item.arch).sort(), ["arm64", "x64"]);
  assert.equal(plan.matrix.include.length, 2);
  assert(plan.matrix.include.every((target) => target.platform === "darwin"));
  assert.throws(() => buildPlan(root, { INPUT_PLATFORM: "windows" }), /Unknown platform/);
  await assert.rejects(
    publishRelease(root, { ...releaseEnv, GITHUB_EVENT_NAME: "workflow_dispatch" }),
    /Only a version-tag push/,
  );
});
test("tag builds require the exact version and build every supported target", async (t) => {
  const root = await fixture(t);
  assert.equal(
    buildPlan(root, { ...releaseEnv, INPUT_PLATFORM: "windows" }).matrix.include.length,
    2,
  );
  assert.throws(
    () => buildPlan(root, { ...releaseEnv, GITHUB_REF_NAME: "v0.2.0" }),
    /matching v0.1.0/,
  );
  assert.throws(
    () => buildPlan(root, { ...releaseEnv, GITHUB_REF_TYPE: "branch" }),
    /matching v0.1.0/,
  );
  assert.throws(() => buildPlan(root, { GITHUB_EVENT_NAME: "pull_request" }), /Unsupported/);
  assert.throws(() => buildPlan(root, { INPUT_PLATFORM: "linux" }), /Unknown platform/);
});
test("version validation rejects inconsistent npm and native metadata", async (t) => {
  for (const file of [
    "package.json",
    "package-lock.json",
    "src-tauri/tauri.conf.json",
    "src-tauri/Cargo.toml",
    "src-tauri/Cargo.lock",
  ]) {
    const root = await fixture(t);
    const text = await readFile(path.join(root, file), "utf8");
    await writeFile(path.join(root, file), text.replaceAll("0.1.0", "0.2.0"));
    assert.throws(() => projectVersion(root), /differs/);
  }
  const root = await fixture(t);
  await writeFile(
    path.join(root, "package-lock.json"),
    JSON.stringify({ version: "0.1.0", packages: { "": { version: "0.2.0" } } }),
  );
  assert.throws(() => projectVersion(root), /npmLockRoot/);
});
test("version validation rejects shell payloads and malformed numeric prereleases", async (t) => {
  for (const version of ["0.1.0;echo unsafe", "01.1.0", "0.1.0-rc.01"]) {
    const root = await fixture(t, version);
    assert.throws(() => projectVersion(root), /valid semantic version/);
  }
});
test("collector accepts one installer, names it by target, and verifies both installers and their checksums", async (t) => {
  const root = await fixture(t);
  await installers(root);
  const files = await verifyAssets(root, "0.1.0");
  assert.equal(files.length, 4);
  assert(files.some((file) => file.endsWith("macos-x64.dmg")));
  assert(!files.some((file) => file.endsWith(".exe")));
  assert(files.some((file) => file.endsWith("macos-arm64.dmg")));
});
test("collector rejects ambiguous or empty package output", async (t) => {
  const root = await fixture(t);
  const folder = path.join(root, "src-tauri/target", targets[0].target, "release/bundle/dmg");
  await mkdir(folder, { recursive: true });
  await writeFile(path.join(folder, "one.dmg"), "");
  await assert.rejects(collectInstaller(root, targets[0].id), /empty/);
  await writeFile(path.join(folder, "two.dmg"), "fixture");
  await assert.rejects(collectInstaller(root, targets[0].id), /exactly one/);
});
test("tampered or unexpected assets cannot reach GitHub release writes", async (t) => {
  const root = await fixture(t);
  await installers(root);
  await writeFile(path.join(root, "artifacts/ClipSeek-Open_0.1.0_macos-arm64.dmg"), "tampered");
  const calls = [];
  await assert.rejects(publishRelease(root, releaseEnv, github(calls)), /Checksum mismatch/);
  assert.equal(calls.length, 0);
  await installers(root);
  await writeFile(path.join(root, "artifacts/private-config.json"), "synthetic unexpected data");
  await assert.rejects(verifyAssets(root, "0.1.0"), /only the supported installers/);
});
test("tag builds create drafts, upload verified assets, and never publish automatically", async (t) => {
  const root = await fixture(t);
  await installers(root);
  const calls = [];
  await publishRelease(root, releaseEnv, github(calls));
  const create = calls.find((args) => args[1] === "create");
  assert(create.includes("--draft"));
  assert(create.includes("--verify-tag"));
  assert(!create.includes("--prerelease"));
  const notes = await readFile(create[create.indexOf("--notes-file") + 1], "utf8");
  assert.match(notes, /Example change/);
  assert.match(notes, /unsigned/);
  assert.equal(calls.filter((args) => args[1] === "upload").length, 1);
});
test("reruns update existing drafts but refuse published releases and moved tags", async (t) => {
  const root = await fixture(t);
  await installers(root);
  const calls = [];
  await publishRelease(root, releaseEnv, github(calls, "draft"));
  assert(!calls.some((args) => args[1] === "create"));
  for (const [state, sha, reason] of [
    ["published", commit, /published release/],
    ["missing", "b".repeat(40), /tag moved/],
  ]) {
    const actions = [];
    await assert.rejects(publishRelease(root, releaseEnv, github(actions, state, sha)), reason);
    assert(!actions.some((args) => ["create", "upload"].includes(args[1])));
  }
});
test("prerelease tags retain draft status and set the prerelease flag", async (t) => {
  const root = await fixture(t, "0.1.0-rc.1");
  await installers(root);
  const calls = [];
  await publishRelease(root, { ...releaseEnv, GITHUB_REF_NAME: "v0.1.0-rc.1" }, github(calls));
  assert(calls.find((args) => args[1] === "create").includes("--prerelease"));
});
test("installer workflow only runs manually or for version tags and pins action revisions", async () => {
  const workflow = parse(await readFile(".github/workflows/installers.yml", "utf8"));
  assert.deepEqual(Object.keys(workflow.on).sort(), ["push", "workflow_dispatch"]);
  assert.deepEqual(workflow.on.push, { tags: ["v*"] });
  assert.deepEqual(workflow.on.workflow_dispatch, {});
  const steps = Object.values(workflow.jobs).flatMap((job) => job.steps);
  assert(
    !steps.some((step) => /Windows|msys2|nsis|prepare-windows-runtime/i.test(JSON.stringify(step))),
  );
  assert.equal(workflow.permissions.contents, "read");
  assert.equal(workflow.jobs.release.permissions.contents, "write");
  assert.match(workflow.jobs.release.if, /github.event_name == 'push'/);
  assert.deepEqual(workflow.jobs.release.needs, ["validate", "build"]);
  assert.equal(workflow.jobs.build.strategy["fail-fast"], false);
  const buildSteps = workflow.jobs.build.steps;
  const e2e = buildSteps.findIndex((step) => step.run === "npm run test:e2e");
  const packaging = buildSteps.findIndex((step) => step.name === "Build macOS installer");
  assert(e2e >= 0 && e2e < packaging, "Native E2E must pass before installer packaging");
  const actions = Object.values(workflow.jobs)
    .flatMap((job) => job.steps)
    .filter((step) => step.uses);
  assert(actions.length > 0);
  for (const step of actions) assert.match(step.uses, /@[a-f0-9]{40}$/);
  const upload = actions.find((step) => step.uses.startsWith("actions/upload-artifact@"));
  assert.equal(upload.with.path, "artifacts/*");
  assert.equal(upload.with["if-no-files-found"], "error");
  assert.equal(upload.with["retention-days"], 14);
});
test("Windows packaging includes the four runtime DLLs at the executable directory", async () => {
  const config = JSON.parse(await readFile("src-tauri/tauri.ci.windows.conf.json", "utf8"));
  const rust = await readFile("src-tauri/src/windows_runtime.rs", "utf8");
  const expected = [...rust.matchAll(/"([A-Z0-9_]+\.dll)"/g)].map((match) => match[1]);
  assert.equal(expected.length, 4);
  assert.deepEqual(Object.values(config.bundle.resources).sort(), expected.sort());
  const prepare = await readFile("scripts/ci/prepare-windows-runtime.ps1", "utf8");
  assert.match(prepare, /VC\/Redist\/MSVC/);
  assert.match(prepare, /14\.44\.35211\.0/);
  assert(!prepare.includes("debug_nonredist"));
});
