# File Tools

桌面端文件工具（Tauri）：Base64 编解码、按大小拆分、分片合并。

## 开发

```bash
npm install --registry=https://registry.npmmirror.com
npm run dev
```

## 打包

### macOS / Linux

```bash
npm run build
```

产物位于 `src-tauri/target/release/bundle/`（macOS 为 `.app` / `.dmg`）。

### Windows (.exe)

在 Mac 上无法直接交叉编译 Windows 安装包。推送代码后，GitHub Actions 会在 Windows 环境自动构建：

1. 打开 [Actions → Build Windows](https://github.com/isfangmk/file-tools/actions/workflows/build-windows.yml)
2. 等待任务完成
3. 在 **Artifacts** 下载安装包：
   - `File-Tools-Windows-Setup` — 在线安装包（体积小，安装时需联网下载 WebView2）
   - `File-Tools-Windows-Setup-Offline` — 离线安装包（约 +127MB，内网可直接安装）
   - `File-Tools-Windows-MSI` — MSI 安装包（可选）

本地 Windows 机器可直接运行：

```bat
build.bat
REM 内网离线包：
npm run build:windows:offline
```

产物：`src-tauri\target\release\bundle\nsis\File Tools_0.1.0_x64-setup.exe`

## 功能

| 功能 | 说明 |
|------|------|
| File → Base64 | 编码后点 Copy 复制到剪贴板（不写 `base64-N.txt`）；拖入文件夹时先压缩为 zip 再编码 |
| Base64 → File | 粘贴剪贴板中的 Base64 文本还原文件并校验 MD5 |
| Split File | 按 MB / KB / Bytes 拆成 `.0001` `.0002` … |
| Merge Files | 选中 `.0001` 自动合并全部后缀分片，并删除分片 |
