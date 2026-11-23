# Testing

Run from the project root:

```sh
npm test
npm run build
npm run quality:check
npm run quality:rust
cargo test --manifest-path src-tauri/Cargo.toml --tests --no-fail-fast
python3 -m unittest discover -s tests/quality -p 'coreml_*_test.py'
```

Tests live in `tests/frontend`, `tests/quality`, and `src-tauri/tests`. Use temporary libraries and synthetic fixtures; preserve source files. Include coverage for library isolation, queue recovery, cancellation, file safety, and search. Never weaken assertions, extend timeouts, or add baseline exceptions to hide failures.

## Model and hardware tests

Models stay outside version control under `CLIPSTUDIO_MODELS/{q8,q4,fp32}`. Prepare a separate synthetic fixture directory containing a red `photo.png`, a 2-second `short.mp4`, a blue 32-second `limit.mp4`, and a 33-second `too-long.mp4`.

```sh
CLIPSTUDIO_MODELS=/absolute/models \
CLIPSTUDIO_EMBEDDING_FIXTURES=/absolute/fixtures \
cargo test --manifest-path src-tauri/Cargo.toml --test embedding_pipeline -- --ignored --nocapture
```

On physical Apple Silicon/macOS 15+, set the same variables plus `CLIPSTUDIO_MOTION_FIXTURE` to a generated changing-frame 32-second video. Run `--test coreml_inference -- --ignored --nocapture --test-threads=1` for all-precision accuracy, long-context, and recovery checks. The threshold remains cosine **>0.9999**. Recovery temporarily renames a generated cache bucket; run serially against that cache. `--test metal_inference -- --ignored --nocapture` independently checks Metal accuracy and heavy-operator placement.

Ignored hardware/model tests count as executed only when explicitly run. Optional resource branches print SKIPPED; `CLIPSTUDIO_CI_REQUIRE_RESOURCES=1` makes missing resources fail. macOS results do not verify Windows. Loopback playback/MCP tests require listener permissions.

## Other fixtures and tools

Media fixtures default to `.local/test-media/`, overridable with `CLIPSTUDIO_TEST_DATA` or test-specific variables. `tests/native/prepare-prores.py` generates playback fixtures; `playback-probe.swift` supports optional WebKit playback tests. `tests/fixtures/generate_exif_demo.py` generates synthetic EXIF fixtures. Keep actual media, fingerprints, and personal paths out of commits.

MCP tests: `--test mcp_http` and `--lib mcp::`; run the ignored `semantic_search` test explicitly with local models. They use temporary configuration and tokens. [CoreML tools](../scripts/coreml/README.md) cover template regeneration and isolated benchmarks. [Installer tests](releases.md) are included in `npm run test:quality`.

## Native desktop E2E (macOS and Windows)

`npm run test:e2e` builds a dedicated debug app and drives its real WKWebView/WebView2 through [WebdriverIO](https://v2.tauri.app/develop/tests/webdriver/). It tests defaults, Unicode library creation, reference imports, duplicate handling, thumbnails, library isolation, model settings, MCP authentication/start/stop, trash/restore/permanent deletion, language persistence, and restoration in a new native process. All three installer jobs run this smoke suite before packaging.

The runner creates a temporary configuration, logs, libraries, and synthetic source files, then removes them. It uses a separate application identifier and `dist-e2e` frontend output, and refuses ordinary binaries. Production assets stay in `dist`. OS file pickers return configured fixture paths; business IPC, events, queues, SQLite, Qdrant, and filesystem operations are real. Animation timing is excluded. Phase results, reports, and failure screenshots stay under ignored `.local/e2e/`; they are not uploaded by the workflow.

To include real Q8 indexing, semantic search, one-vector-per-asset checks, and the 32/33-second boundary:

```sh
CLIPSTUDIO_MODELS=/absolute/models \
CLIPSTUDIO_EMBEDDING_FIXTURES=/absolute/fixtures \
npm run test:e2e:models
```

Use the generated fixture contract above (red photo, 2-second video, blue 32-second video, 33-second rejection). Missing prerequisites fail; the smoke profile does not count as model coverage. All-precision numerical acceptance remains in the separate native model tests. Windows smoke and real-Q8 E2E have been verified on Windows 10 x64. Windows requires a logged-in interactive desktop; an SSH service session alone cannot drive WebView2. Keep temporary test libraries on a volume with adequate free space. Intel desktop E2E still requires a physical macOS 14+ device.

Automation plugins are optional Cargo dependencies behind `e2e`; release builds reject that feature, and normal capabilities exclude automation permissions. `cargo test --manifest-path src-tauri/Cargo.toml --lib --features e2e e2e_isolation -- --test-threads=1` checks fail-closed profile handling without launching a window.

Windows CI checks sandbox isolation with Windows short-path aliases and also runs frontend tests, Rust library tests, command-boundary and embedding contracts, and the Windows runtime, path, icon, and WebView2 suites. CI smoke tests do not download model weights or claim numerical coverage; the resource-required HEIF and real-Q8 suites are separate hardware tests.

### Windows numerical benchmarks

`windows_embedding_benchmark` is an explicitly ignored resource test. Set `CLIPSTUDIO_MODELS`, `CLIPSTUDIO_EMBEDDING_FIXTURES`, and `CLIPSTUDIO_BENCHMARK_OUTPUT`, then run it with `--ignored --nocapture`. To compare an optimization, also set `CLIPSTUDIO_BENCHMARK_REFERENCE` to the original output. Every vector must have 768 finite, normalized values and cosine **>0.9999** against its reference. Keep benchmark files outside version control.

Video preprocessing caches only the immediately preceding pixel-identical input. It retains every time position and feature in the joint video embedding, with one bounded cache per asset. Moving images are still encoded whenever pixels change. Windows CPU sessions use at most four intra-op workers; GPU placement on macOS is unchanged. Synthetic static-video speedups do not establish performance for moving footage.
