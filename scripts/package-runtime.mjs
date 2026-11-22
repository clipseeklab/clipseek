import { createRequire } from "node:module";
import { copyFile, mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { prepareMedia, mediaVersion } from "./build-media.mjs";
import { homedir } from "node:os";
import { assertMachO, runtimeFilePattern } from "./prebuilt-runtime.mjs";
import { ortVersion, ortSource } from "./build-ort.mjs";
const require = createRequire(import.meta.url);
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const arch = process.argv[2];
if (process.platform !== "darwin" || !["arm64", "x64"].includes(arch))
  throw new Error("Usage: node scripts/package-runtime.mjs arm64|x64 [Intel library path]");
const stage = path.join(root, ".local", "dependency-release", arch);
await mkdir(stage, { recursive: true });
const ortRoot = path.dirname(require.resolve("onnxruntime-node/package.json"));
const library =
  arch === "arm64"
    ? path.join(ortRoot, "bin/napi-v6/darwin/arm64", `libonnxruntime.${ortVersion}.dylib`)
    : process.argv[3];
if (!library) throw new Error("Provide the locally compiled Intel library path");
assertMachO(await readFile(library), arch);
await copyFile(library, path.join(stage, "libonnxruntime.dylib"));
// Reuse only the validated native media cache; target-specific metadata is checked below.
if (arch === process.arch) {
  for (const file of await readdir(path.join(root, "runtime")))
    if (file.startsWith("ffmpeg") || file === "ffprobe" || file === "media-build.json")
      await copyFile(path.join(root, "runtime", file), path.join(stage, file));
}
await prepareMedia(root, stage, { arch });
for (const file of ["LICENSE", "ThirdPartyNotices.txt"])
  await copyFile(
    path.join(root, ".cache", `onnxruntime-${ortVersion}-darwin-x64`, "source", file),
    path.join(stage, `onnxruntime-${file}`),
  );
await writeFile(
  path.join(stage, "versions.json"),
  JSON.stringify(
    {
      platform: "darwin",
      arch,
      onnxruntime: ortVersion,
      ffmpeg: mediaVersion,
      ffprobe: mediaVersion,
    },
    null,
    2,
  ),
);
await writeFile(
  path.join(stage, "provenance.json"),
  JSON.stringify(
    {
      onnxruntime: {
        version: ortVersion,
        source: arch === "x64" ? ortSource : { npm: `onnxruntime-node@${ortVersion}` },
        target: arch,
        deploymentTarget: "14.0",
        dependencyArchives: "Canonical GitHub codeload endpoints; unchanged upstream hashes",
      },
      media: JSON.parse(await readFile(path.join(stage, "media-build.json"), "utf8")),
    },
    null,
    2,
  ),
);
for (const file of ["ffmpeg", "ffprobe", "libonnxruntime.dylib"]) {
  const bytes = await readFile(path.join(stage, file));
  assertMachO(bytes, arch);
  if (bytes.includes(homedir()) || bytes.includes(root))
    throw new Error(`Refusing runtime with local paths: ${file}`);
}
const files = await readdir(stage);
if (files.some((file) => !runtimeFilePattern.test(file)))
  throw new Error("Unexpected dependency staging files");
const file = `clipseek-runtime-macos-${arch}.tgz`;
const archive = path.join(root, ".local", "dependency-release", file);
const result = spawnSync(
  "tar",
  [
    "--uid",
    "0",
    "--gid",
    "0",
    "--uname",
    "root",
    "--gname",
    "wheel",
    "-czf",
    archive,
    "-C",
    stage,
    ...files,
  ],
  {
    stdio: "inherit",
    env: { ...process.env, COPYFILE_DISABLE: "1" },
  },
);
if (result.error || result.status !== 0) throw new Error("Runtime packaging failed");
const sha256 = createHash("sha256")
  .update(await readFile(archive))
  .digest("hex");
console.log(JSON.stringify({ arch, file, sha256 }));
