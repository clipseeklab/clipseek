import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFile, mkdir, mkdtemp, rm, readdir, cp, chmod } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const manifestPath = fileURLToPath(new URL("./runtime-manifest.json", import.meta.url));
const tarCommand =
  process.platform === "win32"
    ? path.join(process.env.SystemRoot ?? "C:/Windows", "System32/tar.exe")
    : "tar";

const binaries = ["ffmpeg", "ffprobe", "libonnxruntime.dylib"];
export const windowsRuntimeDlls = [
  "onnxruntime.dll",
  "heif.dll",
  "libde265.dll",
  "DirectML.dll",
  "dxcompiler.dll",
  "dxil.dll",
  "MSVCP140.dll",
  "MSVCP140_1.dll",
  "VCRUNTIME140.dll",
  "VCRUNTIME140_1.dll",
];
export const windowsAppLocalFiles = [
  ...windowsRuntimeDlls.filter((name) => name !== "onnxruntime.dll"),
  "heif-decoder.exe",
  "heif-libheif-COPYING",
  "heif-libde265-COPYING",
];

export async function stageWindowsAppLocalFiles(root) {
  // NSIS deduplicates resource source paths. Use distinct copies for app-local DLLs.
  const output = path.join(root, ".local/windows-runtime");
  await mkdir(output, { recursive: true });
  for (const name of windowsAppLocalFiles)
    await cp(path.join(root, "runtime", name), path.join(output, name));
}

export const runtimeFilePattern =
  /^(ffmpeg(?:\.exe)?|ffprobe(?:\.exe)?|heif-decoder\.exe|heif(?:-lib(?:heif|de265)-COPYING|\.dll)|libde265\.dll|onnxruntime\.dll|DirectML\.dll|dxcompiler\.dll|dxil\.dll|MSVCP140(?:_1)?\.dll|VCRUNTIME140(?:_1)?\.dll|libonnxruntime\.dylib|versions\.json|media-build\.json|provenance\.json|(?:ffmpeg|onnxruntime)-[A-Za-z0-9._-]+)$/;
export function assertMachO(bytes, arch) {
  const cpu = { arm64: 0x100000c, x64: 0x1000007 }[arch];
  if (
    !cpu ||
    bytes.length < 32 ||
    bytes.readUInt32LE(0) !== 0xfeedfacf ||
    bytes.readUInt32LE(4) !== cpu
  )
    throw new Error(`Runtime Mach-O architecture mismatch: expected ${arch}`);
}
export function assertPE(bytes, arch) {
  const error = () => {
    throw new Error(`Runtime PE architecture mismatch: expected ${arch}`);
  };
  if (arch !== "x64" || bytes.length < 64 || bytes.readUInt16LE(0) !== 0x5a4d) error();
  const offset = bytes.readUInt32LE(0x3c);
  if (
    offset + 6 > bytes.length ||
    bytes.readUInt32LE(offset) !== 0x4550 ||
    bytes.readUInt16LE(offset + 4) !== 0x8664
  )
    error();
}
const run = (command, args) => {
  const result = spawnSync(command, args, { stdio: "inherit", shell: false });
  if (result.error || result.status !== 0) throw new Error(`Runtime command failed: ${command}`);
};
async function download(artifact, destination, manifest) {
  if (process.env.GH_TOKEN || process.env.CLIPSEEK_RUNTIME_USE_GH === "1") {
    run("gh", [
      "release",
      "download",
      manifest.tag,
      "--repo",
      manifest.repository,
      "--pattern",
      artifact.file,
      "--dir",
      path.dirname(destination),
      "--clobber",
    ]);
  } else {
    const url = `https://github.com/${manifest.repository}/releases/download/${manifest.tag}/${artifact.file}`;
    run("curl", [
      "--fail",
      "--location",
      "--retry",
      "3",
      "--connect-timeout",
      "20",
      "--output",
      destination,
      url,
    ]);
  }
}
export async function preparePrebuiltRuntime(
  root,
  { platform = process.platform, arch = process.arch, manifest, downloadArtifact = download } = {},
) {
  manifest ??= JSON.parse(await readFile(manifestPath, "utf8"));
  if (!(
    (platform === "darwin" && ["arm64", "x64"].includes(arch)) ||
    (platform === "win32" && arch === "x64")
  ))
    throw new Error(`No prebuilt runtime for ${platform}/${arch}`);
  const windows = platform === "win32";
  const target = windows ? `windows-${arch}` : arch;
  const artifact = manifest.artifacts[target];
  if (
    !artifact ||
    !/^[a-f0-9]{64}$/.test(artifact.sha256) ||
    artifact.file !== `clipseek-runtime-${windows ? "windows" : "macos"}-${arch}.tgz`
  )
    throw new Error(`Missing pinned runtime for ${arch}`);
  manifest = { ...manifest, tag: artifact.tag ?? manifest.tag };
  const cache = path.join(root, ".cache", "prebuilt-runtime", manifest.tag, target);
  await mkdir(cache, { recursive: true });
  const archive = path.join(cache, artifact.file);
  const verified = async () => {
    try {
      return (
        createHash("sha256")
          .update(await readFile(archive))
          .digest("hex") === artifact.sha256
      );
    } catch (error) {
      if (error.code === "ENOENT") return false;
      throw error;
    }
  };
  if (!(await verified())) {
    await rm(archive, { force: true });
    await downloadArtifact(artifact, archive, manifest);
    if (!(await verified())) {
      await rm(archive, { force: true });
      throw new Error("Runtime archive SHA-256 mismatch; refusing extraction");
    }
  }
  const listing = spawnSync(tarCommand, ["-tzf", archive], { encoding: "utf8" });
  const names = listing.stdout?.trim().split(/\r?\n/) ?? [];

  if (listing.status !== 0 || !names.length || names.some((name) => !runtimeFilePattern.test(name)))
    throw new Error("Unexpected runtime archive contents");
  const details = spawnSync(tarCommand, ["-tvzf", archive], { encoding: "utf8" });
  if (
    details.status !== 0 ||
    details.stdout
      .trim()
      .split(/\r?\n/)
      .some((line) => !line.startsWith("-"))
  )
    throw new Error("Runtime archive must contain regular files only");
  const staging = await mkdtemp(path.join(cache, "verified-"));
  try {
    run(tarCommand, ["-xzf", archive, "-C", staging]);
    const info = JSON.parse(await readFile(path.join(staging, "versions.json"), "utf8"));
    if (
      info.platform !== platform ||
      info.arch !== arch ||
      info.onnxruntime !== manifest.onnxruntime ||
      info.ffmpeg !== manifest.ffmpeg ||
      info.ffprobe !== manifest.ffmpeg
    )
      throw new Error("Runtime version or platform mismatch");
    const executableNames = windows
      ? ["ffmpeg.exe", "ffprobe.exe", "heif-decoder.exe"]
      : ["ffmpeg", "ffprobe"];
    const required = windows ? [...executableNames, ...windowsRuntimeDlls] : binaries;
    for (const name of required) {
      const bytes = await readFile(path.join(staging, name));
      if (windows) assertPE(bytes, arch);
      else assertMachO(bytes, arch);
    }
    const output = path.join(root, "runtime");
    await mkdir(output, { recursive: true });
    for (const name of await readdir(staging))
      await cp(path.join(staging, name), path.join(output, name));
    for (const name of executableNames) await chmod(path.join(output, name), 0o755);
    if (windows) await stageWindowsAppLocalFiles(root);
  } finally {
    await rm(staging, { recursive: true, force: true });
  }
}
