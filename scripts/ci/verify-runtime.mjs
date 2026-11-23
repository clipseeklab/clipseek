import { existsSync, readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { assertPE, windowsRuntimeDlls } from "../prebuilt-runtime.mjs";
import { assertIntelLibrary, ortVersion } from "../build-ort.mjs";

if (process.platform !== process.env.BUILD_PLATFORM || process.arch !== process.env.BUILD_ARCH) {
  throw new Error(`Runner ${process.platform}/${process.arch} does not match the installer target`);
}
const root = path.resolve("runtime");
const info = JSON.parse(readFileSync(path.join(root, "versions.json"), "utf8"));
if (info.platform !== process.platform || info.arch !== process.arch)
  throw new Error("Runtime architecture mismatch");
if (info.onnxruntime !== ortVersion) throw new Error("ONNX Runtime version mismatch");
const windows = process.platform === "win32";
const ort = windows ? "onnxruntime.dll" : "libonnxruntime.dylib";
const toolEnv = { ...process.env };
if (windows) {
  // Native tools must load without compiler, Python, or package-manager DLLs on PATH.
  for (const key of Object.keys(toolEnv)) if (key.toUpperCase() === "PATH") delete toolEnv[key];
  toolEnv.PATH = path.join(process.env.SystemRoot ?? "C:/Windows", "System32");
}
const toolOptions = { encoding: "utf8", cwd: root, env: toolEnv };

if (!existsSync(path.join(root, ort))) throw new Error(`Missing ${ort}`);
if (process.platform === "darwin" && process.arch === "x64")
  assertIntelLibrary(path.join(root, ort));
if (windows) {
  for (const file of [...windowsRuntimeDlls, "ffmpeg.exe", "ffprobe.exe", "heif-decoder.exe"])
    assertPE(readFileSync(path.join(root, file)), process.arch);
}
if (windows) {
  const heif = spawnSync(path.join(root, "heif-decoder.exe"), ["--version"], toolOptions);
  if (heif.error || heif.status !== 0 || !heif.stdout.includes("libheif 1.23.5"))
    throw new Error(`HEIF decoder cannot execute or has the wrong version: ${heif.stderr}`);
  for (const license of ["heif-libheif-COPYING", "heif-libde265-COPYING"])
    if (
      !readFileSync(path.join(root, license), "utf8").includes("GNU LESSER GENERAL PUBLIC LICENSE")
    )
      throw new Error(`Missing HEIF dependency license: ${license}`);
}
if ((process.platform === "darwin" && process.arch === "x64") || windows) {
  const result = spawnSync(
    windows ? "python" : "python3",
    [
      "-c",
      "import ctypes,sys,os; dll_dir=os.add_dll_directory(os.path.dirname(sys.argv[1])) if sys.platform=='win32' else None; lib=ctypes.CDLL(sys.argv[1]); lib.OrtGetApiBase.restype=ctypes.c_void_p; api=lib.OrtGetApiBase(); funcs=ctypes.cast(api,ctypes.POINTER(ctypes.c_void_p)); version=ctypes.CFUNCTYPE(ctypes.c_char_p)(funcs[1])().decode(); assert version==sys.argv[2], version",
      path.join(root, ort),
      ortVersion,
    ],
    { encoding: "utf8" },
  );
  if (result.error || result.status !== 0)
    throw new Error(`ONNX Runtime cannot load: ${result.stderr}`);
}
for (const tool of ["ffmpeg", "ffprobe"]) {
  const result = spawnSync(
    path.join(root, tool + (windows ? ".exe" : "")),
    ["-version"],
    toolOptions,
  );
  if (result.error || result.status !== 0 || !result.stdout.includes(info.ffmpeg)) {
    throw new Error(
      `${tool} cannot execute or has the wrong version: ${result.error ?? result.stderr}`,
    );
  }
}
if (process.platform === "darwin" && process.arch === "arm64") {
  const runtime = await import("onnxruntime-node");
  if (
    !runtime.listSupportedBackends().some((backend) => backend.name === "webgpu" && backend.bundled)
  ) {
    throw new Error("Apple Silicon installers require a bundled WebGPU/Metal backend");
  }
  const bundled = path.resolve(
    "node_modules/onnxruntime-node/bin/napi-v6/darwin/arm64/libonnxruntime.1.30.0.dylib",
  );
  if (!readFileSync(bundled).equals(readFileSync(path.join(root, ort)))) {
    throw new Error("Packaged ONNX Runtime differs from the verified WebGPU runtime");
  }
}

const license = spawnSync(path.join(root, "ffmpeg" + (windows ? ".exe" : "")), ["-L"], toolOptions);
if (
  license.status !== 0 ||
  /nonfree parts|not legally redistributable/i.test(license.stdout + license.stderr)
)
  throw new Error("Refusing a non-redistributable media runtime");

console.log(`Native runtime verified for ${process.platform}/${process.arch}`);
