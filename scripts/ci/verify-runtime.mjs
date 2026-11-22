import { existsSync, readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import path from "node:path";

if (process.platform !== process.env.BUILD_PLATFORM || process.arch !== process.env.BUILD_ARCH) {
  throw new Error(`Runner ${process.platform}/${process.arch} does not match the installer target`);
}
const root = path.resolve("runtime");
const info = JSON.parse(readFileSync(path.join(root, "versions.json"), "utf8"));
if (info.platform !== process.platform || info.arch !== process.arch)
  throw new Error("Runtime architecture mismatch");
const windows = process.platform === "win32";
const ort = windows ? "onnxruntime.dll" : "libonnxruntime.dylib";
if (!existsSync(path.join(root, ort))) throw new Error(`Missing ${ort}`);
for (const tool of ["ffmpeg", "ffprobe"]) {
  const result = spawnSync(path.join(root, tool + (windows ? ".exe" : "")), ["-version"], {
    encoding: "utf8",
  });
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
console.log(`Native runtime verified for ${process.platform}/${process.arch}`);
