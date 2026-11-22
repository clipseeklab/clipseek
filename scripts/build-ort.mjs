import { spawnSync } from "node:child_process";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import os from "node:os";
import { createHash } from "node:crypto";

export const ortVersion = "1.30.0";
// Original upstream archive digest recorded by FreeBSD ports commit 02e2be5276:
// https://github.com/freebsd/freebsd-ports/blob/02e2be5276/misc/onnxruntime/distinfo
// The public distcache service uses HTTP; the HTTPS-published digest authenticates
// the complete archive before extraction, including on cache hits.
export const ortSource = Object.freeze({
  url: "http://distcache.freebsd.org/ports-distfiles/onnxruntime/microsoft-onnxruntime-v1.30.0_GH0.tar.gz",
  sha256: "f6681ecbddf53898adf0cc9e8e9e84657485b84d2eca3c8aa353de6d7dd417ef",
});

export function canonicalDependencyUrls(text) {
  return text.replace(
    /https:\/\/github\.com\/([^/]+)\/([^/]+)\/archive\/([^;\s]+?)\.(zip|tar\.gz)/g,
    (_, owner, repository, reference, format) => {
      const revision = /^[a-f0-9]{40}\//.test(reference) ? reference.split("/")[0] : reference;
      return `https://codeload.github.com/${owner}/${repository}/${format}/${revision}`;
    },
  );
}

export function assertIntelLibrary(file) {
  const result = spawnSync("lipo", ["-archs", file], { encoding: "utf8" });
  if (result.error || result.status !== 0 || result.stdout.trim() !== "x86_64")
    throw new Error(`Expected an Intel-only ONNX Runtime library: ${file}`);
}

export async function prepareIntelOrt(root, sourceArtifact = ortSource) {
  const cache = path.join(root, ".cache", `onnxruntime-${ortVersion}-darwin-x64`);
  const source = path.join(cache, "source");
  const build = path.join(cache, "build");
  const output = path.join(build, "Release");
  const marker = path.join(cache, "verified.json");
  const run = (command, args, cwd = cache) => {
    const result = spawnSync(command, args, { cwd, stdio: "inherit", shell: false });
    if (result.error || result.status !== 0)
      throw new Error(`ONNX Runtime build failed: ${command}: ${result.error ?? result.status}`);
  };
  await mkdir(cache, { recursive: true });
  try {
    const info = JSON.parse(await readFile(marker, "utf8"));
    if (
      info.version === ortVersion &&
      info.deploymentTarget === "14.0" &&
      info.sourceSha256 === sourceArtifact.sha256
    ) {
      assertIntelLibrary(path.join(output, `libonnxruntime.${ortVersion}.dylib`));
      return output;
    }
  } catch {
    // Rebuild incomplete or incompatible cached runtimes.
  }
  // Newer npm distributions omit macOS x64. Build the same runtime version;
  // using an older prebuilt library could reject model operators at startup.
  const archive = path.join(cache, "source.tar.gz");
  const verified = async () => {
    try {
      return (
        createHash("sha256")
          .update(await readFile(archive))
          .digest("hex") === sourceArtifact.sha256
      );
    } catch (error) {
      if (error.code === "ENOENT") return false;
      throw error;
    }
  };
  if (!(await verified())) {
    const temporary = archive + ".download";
    await rm(temporary, { force: true });
    run("curl", [
      "--fail",
      "--location",
      "--retry",
      "3",
      "--connect-timeout",
      "20",
      "--output",
      temporary,
      sourceArtifact.url,
    ]);
    const bytes = await readFile(temporary);
    if (createHash("sha256").update(bytes).digest("hex") !== sourceArtifact.sha256) {
      await rm(temporary, { force: true });
      throw new Error("ONNX Runtime source SHA-256 mismatch; refusing extraction");
    }
    await writeFile(archive, bytes);
    await rm(temporary, { force: true });
  }
  await rm(source, { recursive: true, force: true });
  await mkdir(source, { recursive: true });
  run("tar", ["-xzf", archive, "--strip-components=1", "-C", source]);
  const version = (await readFile(path.join(source, "VERSION_NUMBER"), "utf8")).trim();
  if (version !== ortVersion) throw new Error(`Unexpected ONNX Runtime source version: ${version}`);
  const dependencies = path.join(source, "cmake", "deps.txt");
  await writeFile(dependencies, canonicalDependencyUrls(await readFile(dependencies, "utf8")));
  run(
    "bash",
    [
      "build.sh",
      "--config",
      "Release",
      "--build_dir",
      build,
      "--build_shared_lib",
      "--parallel",
      String(Math.min(os.availableParallelism(), 8)),
      "--osx_arch",
      "x86_64",
      "--skip_tests",
      "--no_telemetry",
      "--skip_submodule_sync",
      "--compile_no_warning_as_error",
      "--cmake_extra_defines",
      "CMAKE_OSX_ARCHITECTURES=x86_64",
      "CMAKE_OSX_DEPLOYMENT_TARGET=14.0",
      "onnxruntime_BUILD_UNIT_TESTS=OFF",
      `CMAKE_C_FLAGS=-ffile-prefix-map=${root}=.`,
      `CMAKE_CXX_FLAGS=-ffile-prefix-map=${root}=.`,
    ],
    source,
  );
  assertIntelLibrary(path.join(output, `libonnxruntime.${ortVersion}.dylib`));
  await writeFile(
    marker,
    JSON.stringify({
      version: ortVersion,
      deploymentTarget: "14.0",
      sourceSha256: sourceArtifact.sha256,
    }),
  );
  return output;
}
