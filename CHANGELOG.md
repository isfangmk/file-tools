# Changelog

本项目遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [Unreleased]

## [0.1.0] - 2026-11-27

首次公开发布。

### 功能

- **Encode**：选文件或拖入文件夹（文件夹先压缩为 zip）→ 生成 Base64，
  点 Copy 复制到剪贴板；可多次追加选择文件。
- **Decode**：从剪贴板粘贴 Base64 文本 → 还原为文件并校验 MD5；
  大段文本不进入输入框，只显示文件名、MD5 与大小的摘要。
- **Split**：按指定块大小（MB / KB / Bytes）把文件切成若干块，
  每块生成一份可复制的 Base64。
- **Merge**：选中 `.0001` 形式的分片，自动发现同名前缀的全部后缀分片并合并，
  合并后删除分片。

Base64 文本为三行结构，便于人工检查与拆分：

```
文件名
MD5
Base64 内容
```

### 工程

- CI 增加 lint 作业：Rust 格式检查、Clippy（`-D warnings`）与 Rust 测试。
- CI 增加多平台构建验证：Linux / macOS / Windows。
- 推送 tag 触发发布工作流：构建 Windows 安装包并创建 GitHub Release。
- `cargo fmt` 规范化 Rust 源码；`decode_files_sync` 标注为仅测试使用，
  消除 Clippy 的 dead-code 告警。
- 补充 LICENSE（BSD-3-Clause）与变更记录；界面风格样板移入 `design/`。

[Unreleased]: https://github.com/isfangmk/file-tools/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/isfangmk/file-tools/releases/tag/v0.1.0
