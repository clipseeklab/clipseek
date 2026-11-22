import { spawnSync } from "node:child_process";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

const probe = process.argv[2];
if (!probe) throw new Error("A compiled WebKit probe is required");
const result = spawnSync(probe, [pathToFileURL(resolve("tests/native/probe-smoke.html")).href], {
  encoding: "utf8",
  timeout: 55_000,
});
if (result.error || result.status !== 0)
  throw new Error(`WebKit probe could not run: ${result.error ?? result.stderr}`);
let state;
try { state = JSON.parse(result.stdout); }
catch { throw new Error(`WebKit probe did not return page state: ${result.stdout}\n${result.stderr}`); }
if (state.visibility !== "visible" || !(state.width > 0 && state.height > 0))
  throw new Error(`WebKit probe requires a visible desktop window: ${JSON.stringify(state)}`);
console.log(`[native] WebKit visible-window smoke passed: ${JSON.stringify(state)}`);
