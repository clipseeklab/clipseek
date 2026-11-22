import { spawnSync } from "node:child_process";
import { mkdir, readFile, copyFile, readdir, writeFile, chmod } from "node:fs/promises";
import { createHash } from "node:crypto";
import os from "node:os";
import path from "node:path";

export const mediaVersion = "8.1.2";
const checksum = "464beb5e7bf0c311e68b45ae2f04e9cc2af88851abb4082231742a74d97b524c";
const sourceUrl = "https://deb.debian.org/debian/pool/main/f/ffmpeg/ffmpeg_8.1.2.orig.tar.xz";
const flags = [
  "--disable-autodetect",
  "--disable-everything",
  "--disable-programs",
  "--enable-ffmpeg",
  "--enable-ffprobe",
  "--disable-doc",
  "--disable-debug",
  "--disable-asm",
  "--disable-network",
  "--disable-shared",
  "--enable-static",
  "--enable-protocol=file,pipe",
  "--enable-demuxer=mov,matroska,avi,image2,png_pipe",
  "--enable-decoder=h264,hevc,vp8,vp9,mpeg4,mjpeg,png,bmp",
  "--enable-parser=h264,hevc,vp8,vp9,mpeg4video",
  "--enable-encoder=png",
  "--enable-muxer=image2pipe",
  "--enable-filter=scale,format,transpose,hflip,vflip,buffer,buffersink,null",
  "--enable-swscale",
  "--enable-pthreads",
  "--enable-zlib",
];
const run = (cmd, args, cwd) => {
  const result = spawnSync(cmd, args, { cwd, stdio: "inherit", shell: false });
  if (result.error || result.status !== 0)
    throw new Error(`Command failed: ${cmd}: ${result.error ?? result.status}`);
};
export async function prepareMedia(root, output, { arch = process.arch } = {}) {
  const cross = process.platform === "darwin" && arch !== process.arch;
  const desiredFlags = [
    ...flags,
    ...(cross ? ["--enable-cross-compile", "--target-os=darwin", "--arch=x86_64"] : []),
    ...(process.platform === "darwin"
      ? [
          `--extra-cflags=${cross ? "-arch x86_64 " : ""}-mmacosx-version-min=14.0`,
          `--extra-ldflags=${cross ? "-arch x86_64 " : ""}-mmacosx-version-min=14.0`,
        ]
      : process.platform === "win32"
        ? ["--extra-ldflags=-static"]
        : []),
  ];
  const ext = process.platform === "win32" ? ".exe" : "";
  const existing = spawnSync(path.join(output, "ffmpeg" + ext), ["-version"], {
    encoding: "utf8",
  });
  const provenance = path.join(output, "media-build.json");
  if (existing.status === 0 && existing.stdout.includes(`ffmpeg version ${mediaVersion}`)) {
    try {
      const metadata = JSON.parse(await readFile(provenance, "utf8"));
      if (
        metadata.sha256 === checksum &&
        metadata.platform === process.platform &&
        metadata.arch === arch &&
        JSON.stringify(metadata.flags) === JSON.stringify(desiredFlags)
      )
        return;
    } catch {
      /* First preparation or incomplete prior build. */
    }
  }
  const cache = path.join(root, ".cache");
  await mkdir(cache, { recursive: true });
  const archive = path.join(cache, `ffmpeg-${mediaVersion}.tar.xz`);
  if (process.env.CLIPSTUDIO_FFMPEG_SOURCE_ARCHIVE)
    await copyFile(process.env.CLIPSTUDIO_FFMPEG_SOURCE_ARCHIVE, archive);
  const verified = async () => {
    try {
      return (
        createHash("sha256")
          .update(await readFile(archive))
          .digest("hex") === checksum
      );
    } catch {
      return false;
    }
  };
  if (!(await verified())) {
    run(
      "curl",
      [
        "--fail",
        "--location",
        "--retry",
        "2",
        "--continue-at",
        "-",
        "--output",
        archive,
        sourceUrl,
      ],
      root,
    );
    if (!(await verified()))
      throw new Error("FFmpeg source SHA-256 mismatch. Remove .cache archive and retry.");
  }
  const source = path.join(cache, `ffmpeg-${mediaVersion}`);
  try {
    await readdir(source);
  } catch {
    run("tar", ["-xf", archive, "-C", cache], root);
  }
  // Windows: run with MSYS2 make/gcc/sh/tar/curl on PATH. macOS: Xcode CLI tools.
  try {
    await readFile(path.join(source, "ffbuild/config.mak"));
    run("make", ["clean"], source);
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  run("sh", ["./configure", ...desiredFlags], source);
  run(
    "make",
    ["-j", String(Math.min(os.availableParallelism(), 8)), "ffmpeg" + ext, "ffprobe" + ext],
    source,
  );
  for (const name of ["ffmpeg", "ffprobe"]) {
    const target = path.join(output, name + ext);
    await copyFile(path.join(source, name + ext), target);
    if (process.platform !== "win32") await chmod(target, 0o755);
  }
  for (const name of ["COPYING.LGPLv2.1", "COPYING.LGPLv3", "LICENSE.md"])
    await copyFile(path.join(source, name), path.join(output, `ffmpeg-${name}`));
  const metadata = {
    version: mediaVersion,
    sourceUrl,
    sha256: checksum,
    flags: desiredFlags,
    platform: process.platform,
    arch,
  };
  await writeFile(provenance, JSON.stringify(metadata, null, 2));
  if (cross) return; // The target runner verifies execution and license output.
  const license = spawnSync(path.join(output, "ffmpeg" + ext), ["-L"], {
    encoding: "utf8",
  });
  if (
    license.status !== 0 ||
    /nonfree parts|not legally redistributable/i.test(license.stdout + license.stderr)
  )
    throw new Error("Refusing a non-redistributable media runtime");
}
