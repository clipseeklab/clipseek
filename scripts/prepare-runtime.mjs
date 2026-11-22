import { fileURLToPath } from "node:url";
import path from "node:path";
import { preparePrebuiltRuntime } from "./prebuilt-runtime.mjs";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
await preparePrebuiltRuntime(root);
console.log(`Verified prebuilt runtime: ${path.join(root, "runtime")}`);
