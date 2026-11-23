import path from "node:path";
import { realpathSync } from "node:fs";

export function automationBinary(project, platform = process.platform) {
  if (!["darwin", "win32"].includes(platform))
    throw new Error(`Native E2E is not supported on ${platform}`);
  return path.join(
    project,
    "src-tauri",
    "target",
    "debug",
    `clipstudio-open${platform === "win32" ? ".exe" : ""}`,
  );
}

export function npmInvocation(args, env = process.env, executable = process.execPath) {
  if (!env.npm_execpath) throw new Error("Launch native E2E through npm run test:e2e");
  // Invoke npm's JavaScript entry directly; Windows .cmd launchers require a shell.
  return { command: executable, args: [env.npm_execpath, ...args] };
}

export function sandboxPath(canonicalPath, platform = process.platform) {
  return platform === "win32" ? path.win32.toNamespacedPath(canonicalPath) : canonicalPath;
}

export function canonicalSandboxPath(input) {
  // The native API expands Windows 8.3 aliases, matching Rust canonicalize().
  return sandboxPath(realpathSync.native(input));
}
