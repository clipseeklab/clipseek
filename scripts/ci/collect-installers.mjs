import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { copyFile, mkdir, readdir, stat, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { projectVersion, targets } from "./build-plan.mjs";

export async function sha256(file) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(file)) hash.update(chunk);
  return hash.digest("hex");
}

export async function collectInstaller(root, id) {
  const target = targets.find((item) => item.id === id);
  if (!target) throw new Error(`Unknown installer target: ${id}`);
  const version = projectVersion(root);
  const windows = target.platform === "win32";
  const extension = windows ? ".exe" : ".dmg";
  const folder = path.join(
    root,
    "src-tauri/target",
    target.target,
    "release/bundle",
    windows ? "nsis" : "dmg",
  );
  const installers = (await readdir(folder)).filter((name) => name.endsWith(extension));
  if (installers.length !== 1)
    throw new Error(`Expected exactly one ${extension} installer, found ${installers.length}`);
  const source = path.join(folder, installers[0]);
  if (!(await stat(source)).isFile() || (await stat(source)).size === 0)
    throw new Error("Installer is empty or not a file");
  const output = path.join(root, "artifacts");
  await mkdir(output, { recursive: true });
  const filename = `ClipSeek-Open_${version}_${id}${windows ? "-setup" : ""}${extension}`;
  const destination = path.join(output, filename);
  await copyFile(source, destination);
  await writeFile(
    path.join(output, `SHA256SUMS-${id}.txt`),
    `${await sha256(destination)}  ${filename}\n`,
  );
  return filename;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  console.log(await collectInstaller(process.cwd(), process.env.BUILD_ID));
}
