import path from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../../", import.meta.url));
const output = process.env.CLIPSEEK_E2E_OUTPUT;
if (!process.env.CLIPSEEK_E2E_ROOT || !output) throw new Error("Run E2E through npm run test:e2e");
const app = path.join(root, "src-tauri/target/debug/clipstudio-open");
export const config = {
  runner: "local",
  specs:
    process.env.CLIPSEEK_E2E_PHASE === "bootstrap"
      ? ["./bootstrap.e2e.mjs"]
      : process.env.CLIPSEEK_E2E_PHASE === "restart"
        ? ["./restart.e2e.mjs"]
        : ["./desktop.e2e.mjs"],
  maxInstances: 1,
  framework: "mocha",
  reporters: ["spec"],
  logLevel: "warn",
  outputDir: output,
  bail: 1,
  waitforTimeout: 10000,
  connectionRetryCount: 0,
  mochaOpts: {
    timeout: process.env.CLIPSEEK_E2E_PROFILE === "models" ? 120000 : 30000,
    bail: true,
  },
  services: [
    [
      "@wdio/tauri-service",
      {
        appBinaryPath: app,
        driverProvider: "embedded",
        embeddedPort: Number(process.env.TAURI_WEBDRIVER_PORT),
        captureBackendLogs: true,
        captureFrontendLogs: true,
      },
    ],
  ],
  capabilities: [{ browserName: "tauri", "tauri:options": { application: app } }],
  afterTest: async function (_test, _context, result) {
    if (!result.passed) {
      const { browser } = await import("@wdio/globals");
      await browser.saveScreenshot(path.join(output, "failure.png"));
    }
  },
};
