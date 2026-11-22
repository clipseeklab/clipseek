# Third-party notices

- ClipSeek: original desktop architecture and interface, retained in this independent fork with upstream author attributions.

- EmbeddingGemma 2 ONNX: Google DeepMind / ONNX Community, Apache-2.0. Model files are downloaded separately. Source and model card: https://huggingface.co/onnx-community/embeddinggemma-2-ONNX . Revision and per-file integrity metadata are committed in `src-tauri/model-files.json`.
- Transformers.js 4.3.2: Copyright Hugging Face, Apache-2.0. Gemma 4 patch layout, resize target calculation, and media token assembly in `preprocess.rs` / `engine.rs` are adapted from its public model processor. Source: https://github.com/huggingface/transformers.js . The Apache-2.0 license is included as `LICENSE`. Transformers.js is not bundled with the application.
- CoreML: Apple system framework used for native GPU inference; not redistributed as a separate runtime. CoreMLTools 9.0, ONNX 1.19.1, and NumPy 2.0.2 are optional developer conversion dependencies and are not bundled with the application.
- ONNX Runtime 1.30.0: Copyright Microsoft, MIT. Official npm package `onnxruntime-node`; license/notice files are copied into the runtime bundle.
- FFmpeg 8.1.2: Copyright the FFmpeg developers, LGPL-2.1-or-later for this configuration. Built as separate executables without GPL, version3 or nonfree switches. Corresponding unmodified source archive: https://deb.debian.org/debian/pool/main/f/ffmpeg/ffmpeg_8.1.2.orig.tar.xz . SHA-256: `464beb5e7bf0c311e68b45ae2f04e9cc2af88851abb4082231742a74d97b524c`. Complete build instructions and options are in `scripts/build-media.mjs`; source licenses and `media-build.json` are included in the native bundle. Source code can be downloaded from that public URL and rebuilt with the committed script.
- rmcp 3.3.0: Rust Model Context Protocol SDK, Apache-2.0. Used for the authenticated Streamable HTTP MCP service. Source: https://github.com/modelcontextprotocol/rust-sdk .
- Microsoft Visual C++ x64 runtime: Copyright Microsoft. Windows CI packages unmodified retail runtime DLLs from the hosted Visual Studio installation; Microsoft redistribution terms apply. See https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files .
- Tauri (MIT / Apache-2.0), React (MIT), SQLite (public domain), Qdrant Edge (Apache-2.0), tokenizers (Apache-2.0), image (MIT / Apache-2.0), Lucide (ISC), and other package dependencies retain their upstream licenses. Exact dependency versions are recorded in both lockfiles; package-provided license notices are retained in dependency installations.

The application does not bundle model weights, Node.js, a Python environment, or external application plugins in its source repository.

- WebdriverIO and the WDIO Tauri plugins: MIT, https://github.com/webdriverio/desktop-mobile . Used only by the optional desktop E2E toolchain; automation plugins are excluded from normal application builds.
