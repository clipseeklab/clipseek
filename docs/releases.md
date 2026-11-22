# Installer builds

**Build installers** runs only through `workflow_dispatch` or a pushed `v*` tag. Ordinary commits and pull requests do not build installers.

## Manual builds

1. Push the workflow to the default branch.
2. Open **Actions → Build installers → Run workflow**.
3. Start the workflow; both Apple Silicon and Intel macOS builds run.
4. Download installers and SHA-256 checksums from **Artifacts** within 14 days.

Manual builds never create Releases. Windows installer builds are currently disabled.

| Platform            | Installer |
| ------------------- | --------- |
| macOS Apple Silicon | `.dmg`    |
| macOS Intel         | `.dmg`    |

## Release drafts

Keep version values aligned in package.json, package-lock.json (both entries), Cargo.toml, Cargo.lock, and tauri.conf.json, then update CHANGELOG.md. Push the source commit and its matching tag, for example `v0.1.0`.

Native desktop E2E smoke tests must pass before packaging. Both macOS builds must succeed. The workflow checks versions, checksums, and tag identity before creating a **draft Release**. Review and publish manually. Reruns may replace draft assets but cannot alter a published Release.

## Build requirements

CI uses Node.js 24 and Rust 1.99.0. Public dependencies provide ONNX Runtime and minimal FFmpeg; models download separately. Apple Silicon includes CoreML GPU with Metal/CPU fallback; Intel uses CPU. See [inference](inference.md).

Actions are pinned. Build jobs have read-only permissions; the draft-release job uses GitHub's short-lived token with contents write access. No personal token or SSH key is needed. Installers are currently unsigned and macOS packages are not notarized.

Run a manual cloud build and inspect each installer before tagging a release. Local checks do not verify hosted builds or Intel Mac installation. Workflow and packaging contracts are covered by `npm run test:quality`.

Intel macOS builds compile ONNX Runtime 1.30.0 from the official source tag because the npm package omits x64 binaries. Apple Silicon uses the bundled 1.30.0 library. Runtime preparation checks the Intel Mach-O architecture; its build cache is separate from ARM64. The first Intel build takes longer.
