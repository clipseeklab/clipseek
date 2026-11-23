# Third-party notices

- ClipSeek: original desktop architecture and interface, retained in this independent fork with upstream author attributions.

- EmbeddingGemma 2 ONNX: Google DeepMind / ONNX Community, Apache-2.0. Model files are downloaded separately. Source and model card: https://huggingface.co/onnx-community/embeddinggemma-2-ONNX . Revision and per-file integrity metadata are committed in `src-tauri/model-files.json`.
- Transformers.js 4.3.2: Copyright Hugging Face, Apache-2.0. Gemma 4 patch layout, resize target calculation, and media token assembly in `preprocess.rs` / `engine.rs` are adapted from its public model processor. Source: https://github.com/huggingface/transformers.js . The Apache-2.0 license is included as `LICENSE`. Transformers.js is not bundled with the application.
- CoreML: Apple system framework used for native GPU inference; not redistributed as a separate runtime. CoreMLTools 9.0, ONNX 1.19.1, and NumPy 2.0.2 are optional developer conversion dependencies and are not bundled with the application.
- ONNX Runtime 1.30.0: Copyright Microsoft, MIT. Official npm package `onnxruntime-node`; license/notice files are copied into the runtime bundle.
- FFmpeg 8.1.2: Copyright the FFmpeg developers, LGPL-2.1-or-later for this configuration. Built as separate executables without GPL, version3 or nonfree switches. Corresponding unmodified source archive: https://deb.debian.org/debian/pool/main/f/ffmpeg/ffmpeg_8.1.2.orig.tar.xz . SHA-256: `464beb5e7bf0c311e68b45ae2f04e9cc2af88851abb4082231742a74d97b524c`. Complete build instructions and options are in `scripts/build-media.mjs`; source licenses and `media-build.json` are included in the native bundle. Source code can be downloaded from that public URL and rebuilt with the committed script.
- libheif 1.23.5 and libde265 1.1.3: Copyright their upstream contributors, LGPL-3.0-or-later. Windows decodes HEIF primary images in a separate worker linked to replaceable shared DLLs. Source archives, hashes, and complete build options are recorded in `scripts/build-windows-heif.ps1`; both COPYING files are bundled. Sources: https://github.com/strukturag/libheif/releases/tag/v1.23.5 and https://github.com/strukturag/libde265/releases/tag/v1.1.3 .
- rmcp 3.3.0: Rust Model Context Protocol SDK, Apache-2.0. Used for the authenticated Streamable HTTP MCP service. Source: https://github.com/modelcontextprotocol/rust-sdk .
- Microsoft Visual C++ x64 runtime: Copyright Microsoft. Windows dependency archives include unmodified retail runtime DLLs from a Visual Studio installation; Microsoft redistribution terms apply. See https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files .
- Tauri (MIT / Apache-2.0), React (MIT), SQLite (public domain), Qdrant Edge (Apache-2.0), tokenizers (Apache-2.0), image (MIT / Apache-2.0), Lucide (ISC), and other package dependencies retain their upstream licenses. Exact dependency versions are recorded in both lockfiles; package-provided license notices are retained in dependency installations.

The application does not bundle model weights, Node.js, a Python environment, or external application plugins in its source repository.

- WebdriverIO and the WDIO Tauri plugins: MIT, https://github.com/webdriverio/desktop-mobile . Used only by the optional desktop E2E toolchain; automation plugins are excluded from normal application builds.

## Documentation demo media

README screenshots show an isolated demo library, using NASA images and short video loops generated from them. No personal media or libraries are included. NASA is credited as the image source; no endorsement is implied. See [NASA media usage guidelines](https://www.nasa.gov/nasa-brand-center/images-and-media/).

- [Blue Marble 2012](https://images.nasa.gov/details/GSFC_20171208_Archive_e001386): NASA / NOAA / GSFC / Suomi NPP / VIIRS / Norman Kuring.
- [Earthrise visualization](https://images.nasa.gov/details/GSFC_20171208_Archive_e000496): NASA Goddard.
- [Earthset from Orion](https://images.nasa.gov/details/art002e021278): NASA.
- [Earth observation from the ISS](https://images.nasa.gov/details/iss045e013851): NASA.

These media retain their source terms; the project's Apache-2.0 license does not relicense third-party imagery.
