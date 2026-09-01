# File Tools

桌面端文件工具（Tauri）：Base64 编解码、按大小拆分、分片合并。

## 开发

```bash
npm install --registry=https://registry.npmmirror.com
npm run dev
```

## 打包

```bash
npm run build
```

产物位于 `src-tauri/target/release/bundle/`（macOS 为 `.app` / `.dmg`）。

## 功能

| 功能 | 说明 |
|------|------|
| File → Base64 | 多文件编码为 `base64-N.txt`（文件名 / MD5 / Base64） |
| Base64 → File | 按同样格式还原并校验 MD5 |
| Split File | 按 MB / KB / Bytes 拆成 `.0001` `.0002` … |
| Merge Files | 选中 `.0001` 自动合并全部后缀分片，并删除分片 |
