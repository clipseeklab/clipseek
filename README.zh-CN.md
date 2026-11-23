<h1 align="center">ClipSeek Open</h1>

<p align="center">
  <strong>用一句话找到本地照片和短视频。</strong><br>
  基于 EmbeddingGemma 2 的开源桌面应用。支持文字和参考图片搜索，素材始终在本机处理。
</p>

<p align="center"><a href="README.md">English</a> · 简体中文</p>

<p align="center">
  <a href="https://github.com/clipseeklab/clipseek/releases/tag/v0.1.0"><img src="https://img.shields.io/badge/release-v0.1.0-6658a8" alt="Release v0.1.0"></a>
  <a href="#主要功能"><img src="https://img.shields.io/badge/model-EmbeddingGemma%202-445b93" alt="EmbeddingGemma 2"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-367468" alt="Apache 2.0 license"></a>
</p>

<p align="center">
  <a href="https://github.com/clipseeklab/clipseek/releases/download/v0.1.0/ClipSeek-Open_0.1.0_macos-arm64.dmg"><img src="docs/readme/macos-arm64.svg" width="185" alt="下载 macOS · Apple Silicon · ARM64"></a>
  <a href="https://github.com/clipseeklab/clipseek/releases/download/v0.1.0/ClipSeek-Open_0.1.0_macos-x64.dmg"><img src="docs/readme/macos-x64.svg" width="185" alt="下载 macOS · Intel · x64"></a>
  <a href="https://github.com/clipseeklab/clipseek/releases/download/v0.1.0/ClipSeek-Open_0.1.0_windows-x64-setup.exe"><img src="docs/readme/windows-x64.svg" width="185" alt="下载 Windows · Intel / AMD · x64"></a>
</p>

<p align="center"><a href="#下载-010">下载</a> · <a href="#主要功能">主要功能</a> · <a href="#运行要求">运行要求</a> · <a href="#参与贡献">参与贡献</a></p>

<p align="center">
  <img src="docs/screenshots/library-macos.png" alt="ClipSeek Open 素材库界面，包含太空照片与短视频">
</p>

## 主要功能

- **自然语言搜索**：根据画面内容描述查找照片和视频。
- **以图搜图**：使用参考图片寻找相似画面。
- **短视频检索**：支持最长 **32 秒**的视频，为完整视频构建一个向量，搜索结果返回完整视频。
- **素材管理**：支持文件夹和标签，通过引用原文件入库，不额外复制素材。
- **可选模型精度**：默认使用 **Q8**，同时提供 Q4 和 FP32。
- **MCP 接口**：可按需启用经过身份验证的 [MCP 接口](docs/mcp.md)，让 AI 客户端调用本地素材检索能力。

## 运行要求

| 平台                            | 操作系统                     | 推理方式                                    |
| ------------------------------- | ---------------------------- | ------------------------------------------- |
| Apple Silicon（M1 及更新芯片）  | macOS 14+                    | macOS 15+ 使用 CoreML GPU，可回退 Metal/CPU |
| Intel Mac（64 位）              | macOS 14+                    | CPU                                         |
| Intel/AMD Windows 电脑（64 位） | Windows 10 22H2 / Windows 11 | CPU                                         |

**Q8 推荐配置**：16 GB 内存、SSD，以及 10 GB 可用空间，用于应用、模型和初始缓存。素材缩略图、索引和其他精度的模型需要额外空间。Q8 模型下载约 540 MB，低内存设备尚未验证。

已在 Apple Silicon 和 Windows 10 真机测试；Intel macOS 已通过 GitHub Actions 测试，仍待 macOS 14+ Intel 真机验证。

## 下载 0.1.0

| 平台                | 安装包                                                                                                                     |
| ------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| macOS Apple Silicon | [下载 DMG](https://github.com/clipseeklab/clipseek/releases/download/v0.1.0/ClipSeek-Open_0.1.0_macos-arm64.dmg)           |
| macOS Intel         | [下载 DMG](https://github.com/clipseeklab/clipseek/releases/download/v0.1.0/ClipSeek-Open_0.1.0_macos-x64.dmg)             |
| Windows x64         | [下载安装程序](https://github.com/clipseeklab/clipseek/releases/download/v0.1.0/ClipSeek-Open_0.1.0_windows-x64-setup.exe) |

[更新说明与 SHA-256 校验文件](https://github.com/clipseeklab/clipseek/releases/tag/v0.1.0)。安装包暂未签名，macOS 版本尚未公证。

## 软件截图

![使用自然语言搜索行星与星云的结果](docs/screenshots/search-macos.png)

演示素材包括 NASA 图像和生成的太空插图，详见[素材来源](THIRD_PARTY_NOTICES.md#documentation-demo-media)。

## 开始使用

1. 启动应用，下载模型并创建素材库。
2. 添加本地照片或视频，等待索引完成。
3. 输入场景描述，例如“海边的日落”。

界面默认英文，可在设置中切换中文。在 **设置 → 模型管理** 中更换精度会重建索引。首次下载模型需要联网，之后的素材处理在本地进行。

## 本地开发

安装 Node.js 22.18+、Rust 和 [Tauri 2 开发依赖](https://v2.tauri.app/start/prerequisites/)。

```sh
git clone https://github.com/clipseeklab/clipseek.git
cd clipseek
npm ci
npm run desktop
```

首次运行会下载并校验预构建的 macOS 或 Windows x64 原生依赖；私有仓库需先登录 GitHub CLI，并设置 `CLIPSEEK_RUNTIME_USE_GH=1`。本地打包使用 `npm run desktop:build`；GitHub Actions 支持构建 macOS Apple Silicon / Intel 和 Windows x64 安装包，详见[安装包指南](docs/releases.md)。普通提交不会触发安装包构建。

## 参与贡献

欢迎提交 [Issue](https://github.com/clipseeklab/clipseek/issues) 或 Pull Request。开发前请阅读[测试说明](docs/testing.md)、[代码规范](docs/coding-standards.md)和[架构说明](docs/architecture.md)。不要提交密钥、个人素材、素材库、模型或构建产物。

精度和性能限制见[推理说明](docs/inference.md)。

## 许可证

基于 ClipSeek 的独立项目，采用 [Apache-2.0](LICENSE)。模型适用单独的条款，详见[第三方声明](THIRD_PARTY_NOTICES.md)。
