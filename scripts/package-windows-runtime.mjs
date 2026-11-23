import { copyFile, mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { homedir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  runtimeFilePattern,
  windowsRuntimeDlls,
  stageWindowsAppLocalFiles,
} from "./prebuilt-runtime.mjs";

if (process.env.CI || process.platform !== "win32" || process.arch !== "x64")
  throw new Error("Windows runtime packaging is local x64 maintenance only");
const root = fileURLToPath(new URL("../", import.meta.url));
const runtime = path.join(root, "runtime");
const run = (command, args, options = {}) => {
  const result = spawnSync(command, args, { stdio: "inherit", shell: false, ...options });
  if (result.error || result.status !== 0) throw new Error(`Runtime command failed: ${command}`);
};
run(process.execPath, ["scripts/ci/verify-runtime.mjs"], {
  cwd: root,
  env: { ...process.env, BUILD_PLATFORM: "win32", BUILD_ARCH: "x64" },
});
await stageWindowsAppLocalFiles(root);
const stage = path.join(root, ".local/dependency-release/windows-x64");
await mkdir(stage, { recursive: true });
const binaries = ["ffmpeg.exe", "ffprobe.exe", "heif-decoder.exe", ...windowsRuntimeDlls];
const files = [
  ...binaries,
  "versions.json",
  "media-build.json",
  "onnxruntime-LICENSE",
  "onnxruntime-ThirdPartyNotices.txt",
  "ffmpeg-COPYING.LGPLv2.1",
  "ffmpeg-COPYING.LGPLv3",
  "ffmpeg-LICENSE.md",
  "heif-libheif-COPYING",
  "heif-libde265-COPYING",
];
for (const file of files) {
  const bytes = await readFile(path.join(runtime, file));
  if (!bytes.length) throw new Error(`Empty runtime file: ${file}`);
  if (binaries.includes(file)) {
    for (const local of [homedir(), root])
      for (const value of [local, local.replaceAll("\\", "/")])
        if (bytes.includes(value) || bytes.includes(Buffer.from(value, "utf16le")))
          throw new Error(`Refusing runtime with local paths: ${file}`);
  }
  await copyFile(path.join(runtime, file), path.join(stage, file));
}
const worker = await readFile(path.join(root, "src-tauri/windows/heif-decoder.cpp"));
await writeFile(
  path.join(stage, "provenance.json"),
  JSON.stringify(
    {
      onnxruntime: { npm: "onnxruntime-node@1.30.0", platform: "win32", arch: "x64" },
      media: JSON.parse(await readFile(path.join(stage, "media-build.json"), "utf8")),
      heif: {
        libheif: "1.23.5",
        libde265: "1.1.3",
        recipe: "scripts/build-windows-heif.ps1",
        recipeSha256: createHash("sha256")
          .update(await readFile(path.join(root, "scripts/build-windows-heif.ps1")))
          .digest("hex"),
        workerSha256: createHash("sha256").update(worker).digest("hex"),
      },
    },
    null,
    2,
  ),
);
const entries = (await readdir(stage)).sort();
if (entries.some((name) => !runtimeFilePattern.test(name)) || entries.length !== files.length + 1)
  throw new Error("Unexpected Windows dependency staging files");
const file = "clipseek-runtime-windows-x64.tgz";
const archive = path.join(root, ".local/dependency-release", file);
run(path.join(process.env.SystemRoot ?? "C:/Windows", "System32/tar.exe"), [
  "--uid",
  "0",
  "--gid",
  "0",
  "--uname",
  "root",
  "--gname",
  "root",
  "-czf",
  archive,
  "-C",
  stage,
  ...entries,
]);
const sha256 = createHash("sha256")
  .update(await readFile(archive))
  .digest("hex");
console.log(JSON.stringify({ arch: "x64", file, sha256 }));
