import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { prepareIntelOrt, ortVersion } from "./build-ort.mjs";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const arch = process.argv[2];
if (process.env.CI || process.platform !== "darwin" || !["arm64", "x64"].includes(arch))
  throw new Error("Runtime source builds are local macOS maintenance only; select arm64 or x64");
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
