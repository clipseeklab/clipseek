import test from "node:test";
import { Buffer } from "node:buffer";
import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile, readFile, copyFile, rm, access } from "node:fs/promises";
import { symlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { preparePrebuiltRuntime, assertMachO } from "../../scripts/prebuilt-runtime.mjs";

async function fixture(t, { cpu = 0x1000007, extra = false, version = "1.30.0" } = {}) {
  const base = await mkdtemp(path.join(tmpdir(), "clipseek-prebuilt-"));
  t.after(() => rm(base, { recursive: true, force: true }));
  const stage = path.join(base, "stage");
  await mkdir(stage);
  const header = Buffer.alloc(32);
  header.writeUInt32LE(0xfeedfacf, 0);
  header.writeUInt32LE(cpu, 4);
  for (const name of ["ffmpeg", "ffprobe", "libonnxruntime.dylib"])
    await writeFile(path.join(stage, name), header);
  await writeFile(
    path.join(stage, "versions.json"),
    JSON.stringify({
      platform: "darwin",
      arch: "x64",
      onnxruntime: version,
      ffmpeg: "8.1.2",
      ffprobe: "8.1.2",
    }),
  );
  if (extra) await writeFile(path.join(stage, "unexpected.txt"), "not a runtime");
  const archive = path.join(base, "source.tgz");
  const files = [
    "ffmpeg",
    "ffprobe",
    "libonnxruntime.dylib",
    "versions.json",
    ...(extra ? ["unexpected.txt"] : []),
  ];
  assert.equal(
    spawnSync("tar", ["-czf", archive, "-C", stage, ...files], {
      env: { ...process.env, COPYFILE_DISABLE: "1" },
    }).status,
    0,
  );
  const manifest = {
    tag: "fixture",
    onnxruntime: "1.30.0",
    ffmpeg: "8.1.2",
    artifacts: {
      x64: {
        file: "clipseek-runtime-macos-x64.tgz",
        sha256: createHash("sha256")
          .update(await readFile(archive))
          .digest("hex"),
      },
    },
  };
  const root = path.join(base, "project");
  let downloads = 0;
  const run = (overrides = {}) =>
    preparePrebuiltRuntime(root, {
      platform: "darwin",
      arch: "x64",
      manifest,
      downloadArtifact: async (_, destination) => {
        downloads++;
        await copyFile(archive, destination);
      },
      ...overrides,
    });
  return { base, root, manifest, run, downloads: () => downloads };
}

test("prebuilt download validates the archive, reuses the cache and restores runtime files", async (t) => {
  const f = await fixture(t);
  await f.run();
  await writeFile(path.join(f.root, "runtime/ffmpeg"), "tampered output");
  await f.run();
  assert.equal(f.downloads(), 1);
  assertMachO(await readFile(path.join(f.root, "runtime/ffmpeg")), "x64");
  await writeFile(
    path.join(f.root, ".cache/prebuilt-runtime/fixture/x64/clipseek-runtime-macos-x64.tgz"),
    "tampered cache",
  );
  await f.run();
  assert.equal(f.downloads(), 2);
});

test("a downloaded archive with the wrong digest is rejected before installing files", async (t) => {
  const f = await fixture(t);
  f.manifest.artifacts.x64.sha256 = "0".repeat(64);
  await assert.rejects(f.run(), /SHA-256 mismatch/);
  await assert.rejects(access(path.join(f.root, "runtime")));
});

test("wrong Mach-O architecture is rejected before replacing the runtime", async (t) => {
  const f = await fixture(t, { cpu: 0x100000c });
  await assert.rejects(f.run(), /architecture mismatch/);
  await assert.rejects(access(path.join(f.root, "runtime")));
});

test("wrong runtime version is rejected before installation", async (t) => {
  const f = await fixture(t, { version: "1.22.0" });
  await assert.rejects(f.run(), /version or platform mismatch/);
});

test("unexpected archive files are rejected", async (t) => {
  const f = await fixture(t, { extra: true });
  await assert.rejects(f.run(), /Unexpected runtime archive contents/);
});

test("unsupported platforms have no build fallback", async (t) => {
  const f = await fixture(t);
  await assert.rejects(f.run({ platform: "win32" }), /No prebuilt runtime/);
  assert.equal(f.downloads(), 0);
});

test("runtime archives cannot extract symbolic links", async (t) => {
  const f = await fixture(t);
  const stage = path.join(f.base, "link-stage");
  await mkdir(stage);
  await symlink("/tmp", path.join(stage, "onnxruntime-LICENSE"));
  const archive = path.join(f.base, "link.tgz");
  assert.equal(
    spawnSync("tar", ["-czf", archive, "-C", stage, "onnxruntime-LICENSE"], {
      env: { ...process.env, COPYFILE_DISABLE: "1" },
    }).status,
    0,
  );
  f.manifest.artifacts.x64.sha256 = createHash("sha256")
    .update(await readFile(archive))
    .digest("hex");
  await assert.rejects(
    f.run({ downloadArtifact: async (_, target) => copyFile(archive, target) }),
    /regular files only/,
  );
  await assert.rejects(access(path.join(f.root, "runtime")));
});
