# ClipSeek Open

English · [简体中文](README.zh-CN.md)

Search local photos and videos with natural language or an image. Processing runs on your computer, without an account or cloud service.

- Organize media with folders and tags; original files stay in place.
- Search whole videos up to **32 seconds** long.
- Use EmbeddingGemma 2 with **Q8** by default, or choose Q4 / FP32.
- Use native CoreML GPU on Apple Silicon with macOS 15+, with Metal/CPU fallback. Intel Mac and Windows use CPU.
- Connect AI clients through the optional [MCP interface](docs/mcp.md).

## Getting started

1. Launch the app, download a model, and create a library.
2. Add local photos or videos and wait for indexing.
3. Search for a scene, such as “a sunset by the sea.”

The interface defaults to English; Chinese is available in settings. Change precision in **Settings → Model management**. This rebuilds the index. Initial model download requires internet access; subsequent processing is local.

## Development

Install Node.js 22.18+, Rust, and [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/). Native media builds also need a C/C++ compiler, make, pkg-config, and zlib; use Xcode command line tools on macOS or MSYS2/MinGW on Windows.

```sh
git clone https://github.com/clipseeklab/clipseek.git
cd clipseek
npm ci
npm run desktop
```

The first run prepares native dependencies. Use `npm run desktop:build` to build locally, or follow the [installer guide](docs/releases.md) for on-demand macOS Apple Silicon / Intel GitHub Actions builds. Windows packaging is currently disabled. Ordinary commits do not trigger installer builds.

## Contributing

[Issues](https://github.com/clipseeklab/clipseek/issues) and pull requests are welcome. See [testing](docs/testing.md), [coding guidelines](docs/coding-standards.md), and [architecture](docs/architecture.md). Keep credentials, personal media, libraries, models, and build artifacts out of commits.

Apple Silicon has been tested locally. Intel Mac and Windows still need installer and hardware validation. See [inference notes](docs/inference.md) for accuracy and performance limits.

## License

An independent project based on ClipSeek, licensed under [Apache-2.0](LICENSE). Model terms are separate; see [third-party notices](THIRD_PARTY_NOTICES.md).
