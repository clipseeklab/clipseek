# ClipSeek Open contributor instructions

This is an independent project that retains ClipSeek's Tauri 2 / Rust / React / Zustand / TanStack Query / SQLite / Qdrant Edge architecture and interface. Do not modify the original project. Check Git status and preserve existing changes before editing. Read the [coding guidelines](docs/coding-standards.md), [architecture](docs/architecture.md), and [testing guide](docs/testing.md).

## Language

English is the default for the application, README, changelog, contributor instructions, developer documentation, code comments, logs, and diagnostic messages. Keep `README.md` in English and the Chinese translation in `README.zh-CN.md`. Preserve Chinese localization and deliberate Unicode test fixtures. Honor explicitly saved language preferences; new installations default to English.

## Product constraints

- Import by reference only. Never modify or delete external source files during library management.
- Reject videos longer than 32 seconds or with unknown duration before registration.
- Store one normalized 768-dimensional embedding per photo or whole video.
- Use Q8 by default, with Q4 and FP32 available.
- Do not restore online downloads, trimming, subtitles/ASR, plugin downloads, commercial licensing, or telemetry.
- MCP is disabled by default; when enabled it defaults to loopback and read-only access, sharing existing search and organization services.

## Resources and privacy

Models use `q8`, `q4`, and `fp32` subdirectories under the app-managed directory or `CLIPSTUDIO_MODELS`. Native dependencies live under `runtime`; `npm run runtime:prepare` downloads the pinned dependency Release and verifies archive digests, versions, and native binary architectures. CI must never fall back to building native dependencies from source. `npm run runtime:build` is an explicit local maintenance task. Do not depend on the original project's resource repository. Never commit model weights, native runtimes, libraries, personal media, logs, credentials, or build artifacts. Use generated fixtures or clearly attributed public samples.

## Architecture

Frontend IPC must go through `src/lib/api.ts`. Keep backend snapshots in Query and interaction state in Zustand; scope query keys by library. Preserve session caching, activity guards, queue context, cancellation/retry, and stale-result protection. Never combine queries from one model with vectors produced by another.

## Validation

Test bodies belong in `tests/frontend`, `tests/quality`, or `src-tauri/tests`; business source may only declare external test modules. Do not hide failures by skipping tests, weakening assertions, increasing timeouts, or adding lint exceptions. Distinguish executed resource tests from skipped branches. Frontend business files must stay within 500 lines and Rust files within 700 lines. The quality baseline must remain empty.

Run related tests, `npm run test:typecheck`, `npm run build`, `npm run quality:check`, `npm run quality:rust`, and `git diff --check`. Update documentation and report actual results, unverified platforms, and remaining limitations. Do not publish or upload a commercial release.
