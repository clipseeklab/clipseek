# Installer builds

**Build installers** runs only through `workflow_dispatch` or a pushed `v*` tag. Ordinary commits and pull requests do not build installers.

## Manual builds

1. Push the workflow to the default branch.
2. Open **Actions → Build installers → Run workflow**.
3. Start the workflow; Apple Silicon, Intel macOS, and Windows x64 builds run.
4. Download installers and SHA-256 checksums from **Artifacts** within 14 days.

Manual installer builds never create application Releases. Native dependencies have a separate maintenance Release.

| Platform            | Installer   |
| ------------------- | ----------- |
| macOS Apple Silicon | `.dmg`      |
| macOS Intel         | `.dmg`      |
| Windows x64         | NSIS `.exe` |

## Release drafts

Keep version values aligned in package.json, package-lock.json (both entries), Cargo.toml, Cargo.lock, and tauri.conf.json, then update CHANGELOG.md. Push the source commit and its matching tag, for example `v0.1.0`.

Native desktop E2E smoke tests must pass before packaging. All three builds must succeed. The workflow checks versions, checksums, and tag identity before creating a **draft Release**. Review and publish manually. Reruns may replace draft assets but cannot alter a published Release.

## Build requirements

CI uses Node.js 24 and Rust 1.99.0. Public dependencies provide ONNX Runtime and minimal FFmpeg; models download separately. Apple Silicon includes CoreML GPU with Metal/CPU fallback; Intel and Windows use CPU. See [inference](inference.md).

Actions are pinned. Build jobs have read-only permissions; the draft-release job uses GitHub's short-lived token with contents write access. No personal token or SSH key is needed. Installers are currently unsigned and macOS packages are not notarized.

Run a manual cloud build and inspect each installer before tagging a release. Local checks do not verify hosted builds or Intel Mac installation. Workflow and packaging contracts are covered by `npm run test:quality`.

## Native dependency maintenance

Installer jobs only download the archives pinned by `scripts/runtime-manifest.json`; they verify SHA-256, versions, Mach-O/PE architectures, executable tools, and runtime loading. A failed download or validation aborts the build. There is no source-build fallback.

Maintainers build locally using Python 3.12, CMake 3.31, a C/C++ compiler, make, and Xcode command line tools:

```sh
npm run runtime:build -- x64
npm run runtime:build -- arm64
```

Build Intel first to prepare shared upstream license files. Apple Silicon reuses the verified npm ONNX Runtime, including Metal. Intel compiles the identical runtime version from the original source archive authenticated by FreeBSD ports' SHA-256. Both archives include minimal FFmpeg/FFprobe, licenses, versions, and public provenance. No models or user files are included.

Upload the two `.local/dependency-release/clipseek-runtime-macos-*.tgz` archives and SHA-256 list to a dedicated `runtime-*` dependency Release, marked as a prerelease and never latest. Record their immutable tag and digests in the manifest. Use a new tag for changes; do not replace pinned assets. Dependency tags do not trigger installer workflows. The private-repository workflow uses the short-lived read-only GitHub token; local developers can use `CLIPSEEK_RUNTIME_USE_GH=1` with an authenticated GitHub CLI.

Only a runtime upgrade or build-option change requires rebuilding these dependencies. Validate all native installer jobs after updating the manifest.

### Windows dependency maintenance

Use Windows x64 with Visual Studio C++ tools, CMake/Ninja, Python, and a MinGW64 shell providing make, GCC, tar, curl, and zlib:

```sh
npm ci
npm run runtime:build -- x64
```

The local builder uses the pinned npm ONNX Runtime, compiles minimal FFmpeg and the isolated HEIF worker, and packages retail VC++ DLLs and licenses. Public source archives are checked before extraction. The output is `.local/dependency-release/clipseek-runtime-windows-x64.tgz`; the packager rejects local home/project paths in native binaries. No source builds run in installer CI.

Windows x64 has passed local install, launch, and uninstall validation. Upload its archive and checksum to a separate immutable `runtime-windows-*` prerelease, never latest, and pin its tag and digest under `artifacts.windows-x64` in the manifest. Existing macOS dependency pins stay unchanged. Windows CI runs frontend tests, Rust unit/platform contracts, runtime loading checks, and native desktop E2E before creating an NSIS installer.
