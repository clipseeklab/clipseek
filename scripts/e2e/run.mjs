import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, existsSync, copyFileSync } from "node:fs";
import os from "node:os";
import { assertAutomationBinary, freePort } from "./safety.mjs";
import { automationBinary, npmInvocation, canonicalSandboxPath } from "./platform.mjs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const project = fileURLToPath(new URL("../../", import.meta.url));
const profile = process.argv[2] ?? "smoke";
const binary = automationBinary(project);
if (!["smoke", "models"].includes(profile)) throw new Error("Use smoke or models E2E profile");
const models = process.env.CLIPSTUDIO_MODELS;
const fixtures = process.env.CLIPSTUDIO_EMBEDDING_FIXTURES;
if (profile === "models") {
  if (!models || !fixtures)
    throw new Error("Models E2E requires CLIPSTUDIO_MODELS and CLIPSTUDIO_EMBEDDING_FIXTURES");
  for (const file of [
    path.join(models, "q8/onnx/model_quantized.onnx"),
    ...["photo.png", "short.mp4", "limit.mp4", "too-long.mp4"].map((name) =>
      path.join(fixtures, name),
    ),
  ]) {
    if (!existsSync(file)) throw new Error(`Missing E2E prerequisite: ${file}`);
  }
}
const sandbox = canonicalSandboxPath(mkdtempSync(path.join(os.tmpdir(), "clipseek-e2e-")));
const output = path.join(project, ".local/e2e", path.basename(sandbox));
mkdirSync(output, { recursive: true });
mkdirSync(path.join(sandbox, "config"));
mkdirSync(path.join(sandbox, "libraries"));
mkdirSync(path.join(sandbox, "media"));
copyFileSync(
  path.join(project, "tests/fixtures/exif-demo.png"),
  path.join(sandbox, "media/synthetic 你好.png"),
);
writeFileSync(path.join(sandbox, ".clipseek-e2e-sandbox"), "clipseek-e2e-v1\n");
writeFileSync(
  path.join(sandbox, "config/config.json"),
  JSON.stringify({
    settings: {
      autoVisualIndex: false,
      mcpPort: await freePort(),
      embeddingModelDirectory: profile === "models" ? models : path.join(sandbox, "models"),
    },
    onboarding: { version: 2, currentStep: 3, completed: true },
  }),
);
const env = {
  ...process.env,
  VITE_E2E: "true",
  CLIPSEEK_E2E_ROOT: sandbox,
  CLIPSEEK_E2E_OUTPUT: output,
  CLIPSEEK_E2E_PROFILE: profile,
  CLIPSEEK_E2E_PHASE: "bootstrap",
  TAURI_WEBDRIVER_PORT: String(await freePort()),
};
const phases = [];
function run(args) {
  const invocation = npmInvocation(args);
  const command = invocation.command;
  if (args.includes("wdio")) assertAutomationBinary(binary);
  const result = spawnSync(command, invocation.args, { cwd: project, env, stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.signal) throw new Error(`${command} terminated by ${result.signal}`);
  const status = result.status ?? 1;
  phases.push({
    phase: args.includes("wdio") ? env.CLIPSEEK_E2E_PHASE : "build",
    exitCode: status,
  });
  return status;
}
let code;
try {
  code =
    process.env.CLIPSEEK_E2E_SKIP_BUILD === "1"
      ? 0
      : run([
          "exec",
          "--",
          "tauri",
          "build",
          "--debug",
          "--no-bundle",
          "--features",
          "e2e",
          "--config",
          "tests/frontend/e2e/tauri.conf.json",
        ]);
  if (code === 0) {
    assertAutomationBinary(binary);
    code = run(["exec", "--", "wdio", "run", "tests/frontend/e2e/wdio.conf.mjs"]);
    if (code === 0) {
      env.CLIPSEEK_E2E_PHASE = "journey";
      code = run(["exec", "--", "wdio", "run", "tests/frontend/e2e/wdio.conf.mjs"]);
    }
    if (code === 0) {
      env.CLIPSEEK_E2E_PHASE = "restart";
      code = run(["exec", "--", "wdio", "run", "tests/frontend/e2e/wdio.conf.mjs"]);
    }
  }
} catch (error) {
  code = 1;
  console.error(error instanceof Error ? error.message : String(error));
} finally {
  writeFileSync(
    path.join(output, "result.json"),
    JSON.stringify({ profile, passed: code === 0, exitCode: code, phases }, null, 2) + "\n",
  );
  rmSync(sandbox, { recursive: true, force: true });
}
process.exitCode = code;
