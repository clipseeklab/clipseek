import { createRequire } from "node:module";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFile, mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { ortSource, ortVersion } from "./build-ort.mjs";
import { prepareMedia, mediaVersion } from "./build-media.mjs";

if (process.env.CI || process.platform !== "win32" || process.arch !== "x64")
  throw new Error("Windows source builds are local x64 maintenance only; use a MinGW64 shell");
const root = fileURLToPath(new URL("../", import.meta.url));
const output = path.join(root, "runtime");
const run = (command, args) => {
  const result = spawnSync(command, args, { cwd: root, stdio: "inherit", shell: false });
  if (result.error || result.status !== 0) throw new Error(`Native build failed: ${command}`);
};
await mkdir(output, { recursive: true });
const require = createRequire(import.meta.url);
const npmRoot = path.dirname(require.resolve("onnxruntime-node/package.json"));
if (JSON.parse(await readFile(path.join(npmRoot, "package.json"), "utf8")).version !== ortVersion)
  throw new Error("Installed ONNX Runtime differs from the pinned version");
for (const name of ["onnxruntime.dll", "DirectML.dll", "dxcompiler.dll", "dxil.dll"])
  await copyFile(path.join(npmRoot, "bin/napi-v6/win32/x64", name), path.join(output, name));
const cache = path.join(root, ".cache/onnxruntime-windows-notices");
await mkdir(cache, { recursive: true });
const archive = path.join(cache, "source.tar.gz");
if (process.env.CLIPSTUDIO_ORT_SOURCE_ARCHIVE)
  await copyFile(process.env.CLIPSTUDIO_ORT_SOURCE_ARCHIVE, archive);
const verified = async () => {
  try {
    return (
      createHash("sha256")
        .update(await readFile(archive))
        .digest("hex") === ortSource.sha256
    );
  } catch (error) {
    if (error.code === "ENOENT") return false;
    throw error;
  }
};
if (!(await verified())) {
  run("curl", ["--fail", "--location", "--retry", "3", "--output", archive, ortSource.url]);
  if (!(await verified())) throw new Error("ONNX Runtime notice source SHA-256 mismatch");
}
const prefix = `onnxruntime-${ortVersion}`;
for (const name of ["LICENSE", "ThirdPartyNotices.txt"]) {
  run("tar", [
    "-xzf",
    path.relative(root, archive),
    "--strip-components=1",
    "-C",
    path.relative(root, cache),
    `${prefix}/${name}`,
  ]);
  await copyFile(path.join(cache, name), path.join(output, `onnxruntime-${name}`));
}
run("powershell.exe", [
  "-NoLogo",
  "-NoProfile",
  "-NonInteractive",
  "-ExecutionPolicy",
  "Bypass",
  "-File",
  "scripts/ci/prepare-windows-runtime.ps1",
]);
for (const name of ["MSVCP140.dll", "MSVCP140_1.dll", "VCRUNTIME140.dll", "VCRUNTIME140_1.dll"])
  await copyFile(path.join(root, ".local/windows-runtime", name), path.join(output, name));
await prepareMedia(root, output);
run("powershell.exe", [
  "-NoLogo",
  "-NoProfile",
  "-NonInteractive",
  "-ExecutionPolicy",
  "Bypass",
  "-File",
  "scripts/build-windows-heif.ps1",
]);
await writeFile(
  path.join(output, "versions.json"),
  JSON.stringify(
    {
      platform: "win32",
      arch: "x64",
      onnxruntime: ortVersion,
      ffmpeg: mediaVersion,
      ffprobe: mediaVersion,
    },
    null,
    2,
  ),
);
run(process.execPath, ["scripts/package-windows-runtime.mjs"]);
