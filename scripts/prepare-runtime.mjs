import { createRequire } from "node:module";
import { cp, mkdir, readdir, copyFile, chmod, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { prepareMedia, mediaVersion } from "./build-media.mjs";
import { prepareIntelOrt, ortVersion } from "./build-ort.mjs";
const require = createRequire(import.meta.url);
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = path.join(root, "runtime");
await mkdir(output, { recursive: true });
const ortRoot = path.dirname(require.resolve("onnxruntime-node/package.json"));
const binaryDir = path.join(ortRoot, "bin", "napi-v6", process.platform, process.arch);
const intelMac = process.platform === "darwin" && process.arch === "x64";
const sourceDir = intelMac ? await prepareIntelOrt(root) : binaryDir;
const files = await readdir(sourceDir);
const ortName =
  process.platform === "win32"
    ? "onnxruntime.dll"
    : process.platform === "darwin"
      ? "libonnxruntime.dylib"
      : "libonnxruntime.so";
const library = files.find((name) =>
  process.platform === "darwin"
    ? /^libonnxruntime\.\d+\.\d+\.\d+\.dylib$/.test(name)
    : process.platform === "linux"
      ? /^libonnxruntime\.so\.\d/.test(name)
      : name === ortName,
);
if (!library) throw new Error(`No ONNX Runtime for ${process.platform}/${process.arch}`);
await copyFile(path.join(sourceDir, library), path.join(output, ortName));
for (const name of files.filter(
  (name) => name.includes("providers_shared") || name === "DirectML.dll",
))
  await copyFile(path.join(sourceDir, name), path.join(output, name));
await prepareMedia(root, output);
for (const file of await readdir(ortRoot))
  if (/license|notice/i.test(file))
    await cp(path.join(ortRoot, file), path.join(output, `onnxruntime-${file}`));
await writeFile(
  path.join(output, "versions.json"),
  JSON.stringify(
    {
      platform: process.platform,
      arch: process.arch,
      onnxruntime: ortVersion,
      ffmpeg: mediaVersion,
      ffprobe: mediaVersion,
    },
    null,
    2,
  ),
);
console.log(`Prepared standalone runtime: ${output}`);
