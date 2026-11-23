import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { prepareIntelOrt, ortVersion } from "./build-ort.mjs";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const arch = process.argv[2];
if (process.env.CI) throw new Error("Runtime source builds are local maintenance only");
if (process.platform === "win32" && arch === "x64") {
  const result = spawnSync(
    process.execPath,
    [path.join(root, "scripts/build-windows-runtime.mjs")],
    {
      stdio: "inherit",
    },
  );
  if (result.error || result.status !== 0) throw new Error("Local Windows runtime build failed");
} else if (process.platform !== "darwin" || !["arm64", "x64"].includes(arch)) {
  throw new Error("Select a supported local runtime: macOS arm64/x64 or Windows x64");
} else {
  const library =
    arch === "x64"
      ? path.join(await prepareIntelOrt(root), `libonnxruntime.${ortVersion}.dylib`)
      : undefined;
  const result = spawnSync(
    process.execPath,
    [path.join(root, "scripts/package-runtime.mjs"), arch, ...(library ? [library] : [])],
    { stdio: "inherit" },
  );
  if (result.error || result.status !== 0) throw new Error("Local runtime packaging failed");
}
