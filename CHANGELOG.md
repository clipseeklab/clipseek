# Changelog

## 0.1.0

- Local photo and whole-video search with EmbeddingGemma 2; videos limited to 32 seconds.
- Q8 by default, with Q4/FP32 options and compact model management.
- Native CoreML GPU on Apple Silicon/macOS 15+, with Metal/CPU fallback.
- Reference-only libraries, folders, tags, previews, and durable import/index queues.
- Native desktop E2E with isolated profiles, real Q8 search coverage, and process restart checks.
- Fixed library-scoped asset listings and visual-model readiness after indexing.
- Optional authenticated MCP interface sharing desktop search and library services.
- English-first interface and documentation, with Chinese translations.
- On-demand macOS ARM64/Intel installer builds; version tags create Release drafts.

Retains ClipSeek's interface, state management, library sessions, and search infrastructure in an independent project. Online downloads, trimming, subtitles, activation, plugins, telemetry, and automatic updates are excluded.
