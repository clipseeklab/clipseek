import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFile, mkdir, mkdtemp, rm, readdir, cp, chmod } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const manifestPath = fileURLToPath(new URL("./runtime-manifest.json", import.meta.url));
const binaries = ["ffmpeg", "ffprobe", "libonnxruntime.dylib"];
export const runtimeFilePattern =
  /^(ffmpeg|ffprobe|libonnxruntime\.dylib|versions\.json|media-build\.json|provenance\.json|(?:ffmpeg|onnxruntime)-[A-Za-z0-9._-]+)$/;
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
  if (platform !== "darwin" || !["arm64", "x64"].includes(arch))
    throw new Error(`No prebuilt runtime for ${platform}/${arch}`);
  const artifact = manifest.artifacts[arch];
  if (
    !artifact ||
    !/^[a-f0-9]{64}$/.test(artifact.sha256) ||
    artifact.file !== `clipseek-runtime-macos-${arch}.tgz`
  )
    throw new Error(`Missing pinned runtime for ${arch}`);
  const cache = path.join(root, ".cache", "prebuilt-runtime", manifest.tag, arch);
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
  const listing = spawnSync("tar", ["-tzf", archive], { encoding: "utf8" });
  const names = listing.stdout?.trim().split("\n") ?? [];

  if (listing.status !== 0 || !names.length || names.some((name) => !runtimeFilePattern.test(name)))
    throw new Error("Unexpected runtime archive contents");
  const details = spawnSync("tar", ["-tvzf", archive], { encoding: "utf8" });
  if (
    details.status !== 0 ||
    details.stdout
      .trim()
      .split("\n")
      .some((line) => !line.startsWith("-"))
  )
    throw new Error("Runtime archive must contain regular files only");
  const staging = await mkdtemp(path.join(cache, "verified-"));
  try {
    run("tar", ["-xzf", archive, "-C", staging]);
    const info = JSON.parse(await readFile(path.join(staging, "versions.json"), "utf8"));
    if (
      info.platform !== platform ||
      info.arch !== arch ||
      info.onnxruntime !== manifest.onnxruntime ||
      info.ffmpeg !== manifest.ffmpeg ||
      info.ffprobe !== manifest.ffmpeg
    )
      throw new Error("Runtime version or platform mismatch");
    for (const name of binaries) assertMachO(await readFile(path.join(staging, name)), arch);
    const output = path.join(root, "runtime");
    await mkdir(output, { recursive: true });
    for (const name of await readdir(staging))
      await cp(path.join(staging, name), path.join(output, name));
    for (const name of ["ffmpeg", "ffprobe"]) await chmod(path.join(output, name), 0o755);
  } finally {
    await rm(staging, { recursive: true, force: true });
  }
}
