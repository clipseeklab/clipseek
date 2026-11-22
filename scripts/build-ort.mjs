import { spawnSync } from "node:child_process";
import { mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import path from "node:path";

export const ortVersion = "1.30.0";
const repository = "https://github.com/microsoft/onnxruntime.git";

export function assertIntelLibrary(file) {
  const result = spawnSync("lipo", ["-archs", file], { encoding: "utf8" });
  if (result.error || result.status !== 0 || result.stdout.trim() !== "x86_64")
    throw new Error(`Expected an Intel-only ONNX Runtime library: ${file}`);
}

export async function prepareIntelOrt(root) {
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
    if (info.version === ortVersion && info.deploymentTarget === "14.0") {
      assertIntelLibrary(path.join(output, `libonnxruntime.${ortVersion}.dylib`));
      return output;
    }
  } catch {
    // Rebuild incomplete or incompatible cached runtimes.
  }
  // Newer npm distributions omit macOS x64. Build the same runtime version;
  // using an older prebuilt library could reject model operators at startup.
  try {
    await readdir(source);
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
    run("git", ["clone", "--depth", "1", "--branch", `v${ortVersion}`, repository, source]);
  }
  const version = (await readFile(path.join(source, "VERSION_NUMBER"), "utf8")).trim();
  if (version !== ortVersion) throw new Error(`Unexpected ONNX Runtime source version: ${version}`);
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
      "2",
      "--skip_tests",
      "--compile_no_warning_as_error",
      "--cmake_extra_defines",
      "CMAKE_OSX_ARCHITECTURES=x86_64",
      "CMAKE_OSX_DEPLOYMENT_TARGET=14.0",
      "onnxruntime_BUILD_UNIT_TESTS=OFF",
    ],
    source,
  );
  assertIntelLibrary(path.join(output, `libonnxruntime.${ortVersion}.dylib`));
  await writeFile(marker, JSON.stringify({ version: ortVersion, deploymentTarget: "14.0" }));
  return output;
}
