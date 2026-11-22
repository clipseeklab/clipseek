import { execFileSync } from "node:child_process";
import { lstat, readFile, readdir, writeFile, mkdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { buildPlan, targets } from "./build-plan.mjs";
import { sha256 } from "./collect-installers.mjs";

export async function verifyAssets(root, version) {
  const folder = path.join(root, "artifacts");
  const expected = [];
  for (const target of targets) {
    const windows = target.platform === "win32";
    const name = `ClipSeek-Open_${version}_${target.id}${windows ? "-setup.exe" : ".dmg"}`;
    const checksum = `SHA256SUMS-${target.id}.txt`;
    expected.push(name, checksum);
    for (const file of [name, checksum]) {
      const info = await lstat(path.join(folder, file));
      if (!info.isFile() || !info.size)
        throw new Error(`Missing, empty, or unsafe release asset: ${file}`);
    }
    const actual = await sha256(path.join(folder, name));
    if ((await readFile(path.join(folder, checksum), "utf8")) !== `${actual}  ${name}\n`) {
      throw new Error(`Checksum mismatch: ${name}`);
    }
  }
  const found = await readdir(folder);
  if (found.length !== expected.length || found.some((name) => !expected.includes(name))) {
    throw new Error(
      "Release directory must contain only the supported installers and their checksums",
    );
  }
  return expected.map((name) => path.join(folder, name));
}

function gh(args) {
  return execFileSync("gh", args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
}

export async function publishRelease(root, env = process.env, execute = gh) {
  if (env.GITHUB_EVENT_NAME !== "push")
    throw new Error("Only a version-tag push can create a release draft");
  const plan = buildPlan(root, env);
  const repository = env.GITHUB_REPOSITORY;
  if (!/^[\w.-]+\/[\w.-]+$/.test(repository ?? "")) throw new Error("Invalid repository");
  if (!/^[a-f0-9]{40,64}$/.test(env.GITHUB_SHA ?? "")) throw new Error("Invalid source commit");
  const files = await verifyAssets(root, plan.version);
  const tag = `v${plan.version}`;
  const current = execute([
    "api",
    `repos/${repository}/commits/${encodeURIComponent(tag)}`,
    "--jq",
    ".sha",
  ]);
  if (current !== env.GITHUB_SHA) throw new Error("The version tag moved after this build started");
  let existing;
  try {
    existing = JSON.parse(
      execute(["release", "view", tag, "--repo", repository, "--json", "isDraft"]),
    );
  } catch (error) {
    const message = String(error.stderr ?? error.message);
    if (!/release not found|HTTP 404|Not Found/i.test(message)) throw error;
  }
  if (existing && existing.isDraft !== true)
    throw new Error("Refusing to modify an already published release");
  if (!existing) {
    const changelog = await readFile(path.join(root, "CHANGELOG.md"), "utf8");
    const section = changelog
      .split(/^## /m)
      .find((part) => part.split(/\r?\n/, 1)[0] === plan.version);
    if (!section) throw new Error(`Missing changelog entry for ${plan.version}`);
    const output = path.join(root, ".local/ci");
    await mkdir(output, { recursive: true });
    const notes = path.join(output, "release-notes.md");
    await writeFile(
      notes,
      `${section.slice(plan.version.length).trim()}\n\nSource commit: ${env.GITHUB_SHA}\n\nThese installers are unsigned and macOS is not notarized. Model weights download separately on first use. Review the installers before publishing this draft.\n`,
    );
    const args = [
      "release",
      "create",
      tag,
      "--repo",
      repository,
      "--verify-tag",
      "--draft",
      "--title",
      `ClipSeek Open ${plan.version}`,
      "--notes-file",
      notes,
    ];
    if (plan.prerelease) args.push("--prerelease");
    execute(args);
  }
  execute(["release", "upload", tag, ...files, "--repo", repository, "--clobber"]);
  console.log(`Installers uploaded to draft release ${tag}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await publishRelease(process.cwd());
}
