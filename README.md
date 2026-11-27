# File Tools

[![CI](https://github.com/isfangmk/file-tools/actions/workflows/ci.yml/badge.svg)](https://github.com/isfangmk/file-tools/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/isfangmk/file-tools.svg?maxAge=86400)](https://github.com/isfangmk/file-tools/releases/latest)
[![License](https://img.shields.io/badge/license-BSD--3--Clause-blue.svg)](LICENSE)

桌面端文件工具（Tauri v2）：Base64 编解码、按块拆分、分片合并。

拖入文件即可处理，结果直接复制到剪贴板，无需在应用内外来回切换。

---

## 功能

| 模式 | 说明 |
|------|------|
| **Encode** | 选文件或拖入文件夹（文件夹先压缩为 zip）→ 生成 Base64，点 Copy 复制到剪贴板 |
| **Decode** | 从剪贴板粘贴 Base64 文本 → 还原为文件，校验 MD5；大文本不进入输入框，只显示摘要 |
| **Split** | 按指定块大小（MB / KB / Bytes）把文件切成若干块，每块生成一份可复制的 Base64 |
| **Merge** | 选中 `.0001` 形式的分片，自动发现同名前缀的全部后缀分片并合并，合并后删除分片 |

Base64 文本为三行结构，便于人工检查与拆分：

```
文件名
MD5
Base64 内容
```

## 下载

Windows 安装包在 [Releases](https://github.com/isfangmk/file-tools/releases/latest)：

| 产物 | 说明 |
|------|------|
| `File-Tools-Windows-Setup-<版本>.exe` | 在线安装包，体积小，安装时需联网下载 WebView2 |
| `File-Tools-Windows-Setup-Offline-<版本>.exe` | 离线安装包（自带 WebView2），内网可直接安装 |
| `File-Tools-Windows-<版本>.msi` | MSI 安装包 |

macOS / Linux 可自行构建（见下）。

应用完全离线运行，不收集任何数据、不发起任何网络请求。

## 开发

需要 [Node.js](https://nodejs.org/) 与 [Rust](https://www.rust-lang.org/tools/install)。

```bash
npm install
npm run dev
```

内网环境可用国内镜像：

```bash
npm install --registry=https://registry.npmmirror.com
```

## 构建

```bash
npm run build
```

产物位于 `src-tauri/target/release/bundle/`（macOS 为 `.app` / `.dmg`）。

在 Mac 上无法直接交叉编译 Windows 安装包，Windows 构建由 GitHub Actions 完成：

```bash
# 本地 Windows
build.bat

# 内网离线包
npm run build:windows:offline
```

## 技术栈

- [Tauri v2](https://tauri.app/) — 桌面外壳
- Rust — 文件处理（编码、拆分、合并、MD5、zip）
- 原生前端（无框架）— 单页界面

## 目录结构

```
src/                  前端（index.html / main.js / styles.css）
src-tauri/            Rust 端
  src/lib.rs          业务实现与 Tauri 命令
  tauri.conf.json     应用配置
  tauri.offline.conf.json  离线安装包配置
design/               界面风格样板
tools/                仓库辅助脚本
.github/workflows/    CI 与发布
```

## 许可

[BSD-3-Clause](LICENSE)
