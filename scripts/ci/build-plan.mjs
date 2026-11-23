import { appendFileSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const targets = [
  {
    id: "macos-arm64",
    os: "macos-15",
    platform: "darwin",
    arch: "arm64",
    target: "aarch64-apple-darwin",
    bundles: "dmg",
  },
  {
    id: "macos-x64",
    os: "macos-15-intel",
    platform: "darwin",
    arch: "x64",
    target: "x86_64-apple-darwin",
    bundles: "dmg",
  },
  {
    id: "windows-x64",
    os: "windows-2025",
    platform: "win32",
    arch: "x64",
    target: "x86_64-pc-windows-msvc",
    bundles: "nsis",
  },
];

export function projectVersion(root) {
  const json = (name) => JSON.parse(readFileSync(path.join(root, name), "utf8"));
  const text = (name) => readFileSync(path.join(root, name), "utf8");
  const npm = json("package.json");
  const lock = json("package-lock.json");
  const cargo = text("src-tauri/Cargo.toml").match(/\[package\]([\s\S]*?)(?=\n\[|$)/)?.[1];
  const cargoLock = text("src-tauri/Cargo.lock")
    .split("[[package]]")
    .find((block) => /^name = "clipstudio-open"$/m.test(block));
  const versions = {
    npm: npm.version,
    npmLock: lock.version,
    npmLockRoot: lock.packages?.[""]?.version,
    cargo: cargo?.match(/^version = "([^"]+)"$/m)?.[1],
    cargoLock: cargoLock?.match(/^version = "([^"]+)"$/m)?.[1],
    tauri: json("src-tauri/tauri.conf.json").version,
  };
  const semver =
    /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([\da-zA-Z-]+(?:\.[\da-zA-Z-]+)*))?(?:\+[\da-zA-Z-]+(?:\.[\da-zA-Z-]+)*)?$/;
  const match = typeof npm.version === "string" && npm.version.match(semver);
  if (!match || match[4]?.split(".").some((part) => /^0\d+$/.test(part))) {
    throw new Error("package.json must contain a valid semantic version");
  }
  for (const [source, version] of Object.entries(versions)) {
    if (version !== npm.version)
      throw new Error(`${source} version ${version} differs from ${npm.version}`);
  }
  return npm.version;
}

export function buildPlan(root, env = process.env) {
  const version = projectVersion(root);
  const event = env.GITHUB_EVENT_NAME ?? "workflow_dispatch";
  if (!["push", "workflow_dispatch"].includes(event))
    throw new Error(`Unsupported build event: ${event}`);
  const release = event === "push";
  if (release && (env.GITHUB_REF_TYPE !== "tag" || env.GITHUB_REF_NAME !== `v${version}`)) {
    throw new Error(`Release builds require a tag matching v${version}`);
  }
  const platform = release ? "all" : (env.INPUT_PLATFORM ?? "all");
  if (!["all", "macos", "windows"].includes(platform))
    throw new Error(`Unknown platform: ${platform}`);
  const include = targets.filter(
    (target) => platform === "all" || target.id.startsWith(`${platform}-`),
  );
  return { version, matrix: { include }, prerelease: version.includes("-") };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const plan = buildPlan(process.cwd());
  if (process.env.GITHUB_OUTPUT) {
    appendFileSync(
      process.env.GITHUB_OUTPUT,
      `version=${plan.version}\nmatrix=${JSON.stringify(plan.matrix)}\nprerelease=${plan.prerelease}\n`,
    );
  }
  console.log(JSON.stringify(plan, null, 2));
}
