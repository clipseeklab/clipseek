# ClipSeek Open

[English](README.md) · 简体中文

用自然语言或图片搜索本地照片和视频。素材处理在本机完成，无需账号或云服务。

- 通过文件夹和标签管理素材，原文件保留在原位置。
- 支持最长 **32 秒**的视频，搜索结果返回完整视频。
- 使用 EmbeddingGemma 2，默认 **Q8**，可选 Q4 / FP32。
- Apple Silicon 在 macOS 15+ 使用原生 CoreML GPU，可回退 Metal/CPU；Intel Mac 和 Windows 使用 CPU。
- 可通过 [MCP 接口](docs/mcp.md)连接 AI 客户端。

## 开始使用

1. 启动应用，下载模型并创建素材库。
2. 添加本地照片或视频，等待索引完成。
3. 输入场景描述，例如“海边的日落”。

界面默认英文，可在设置中切换中文。在**设置 → 模型管理**中更换精度会重建索引。首次下载模型需要联网，之后的素材处理在本地进行。

## 本地开发

安装 Node.js 22.18+、Rust 和 [Tauri 2 开发依赖](https://v2.tauri.app/start/prerequisites/)。

```sh
git clone https://github.com/clipseeklab/clipseek.git
cd clipseek
npm ci
npm run desktop
```

首次运行会下载并校验预构建的 macOS 原生依赖；私有仓库需先登录 GitHub CLI，并设置 `CLIPSEEK_RUNTIME_USE_GH=1`。本地打包使用 `npm run desktop:build`；GitHub Actions 当前仅构建 macOS Apple Silicon / Intel 安装包，Windows 打包暂不启用，详见[安装包指南](docs/releases.md)。普通提交不会触发安装包构建。

## 参与贡献

欢迎提交 [Issue](https://github.com/clipseeklab/clipseek/issues) 或 Pull Request。开发前请阅读[测试说明](docs/testing.md)、[代码规范](docs/coding-standards.md)和[架构说明](docs/architecture.md)。不要提交密钥、个人素材、素材库、模型或构建产物。

已在 Apple Silicon 本机测试；Intel Mac 和 Windows 仍需安装包及实机验证。精度和性能限制见[推理说明](docs/inference.md)。

## 许可证

基于 ClipSeek 的独立项目，采用 [Apache-2.0](LICENSE)。模型适用单独的条款，详见[第三方声明](THIRD_PARTY_NOTICES.md)。
