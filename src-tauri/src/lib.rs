//! File Tools 后端：Base64 编解码、文件拆分与合并。

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use md5::{Digest, Md5};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::CompressionMethod;
use zip::ZipWriter;

/// 将字节数格式化为人类可读字符串。
fn fsize(b: u64) -> String {
    if b < 1024 {
        format!("{b} B")
    } else if b < 1_048_576 {
        format!("{:.1} KB", b as f64 / 1024.0)
    } else if b < 1_073_741_824 {
        format!("{:.2} MB", b as f64 / 1_048_576.0)
    } else {
        format!("{:.2} GB", b as f64 / 1_073_741_824.0)
    }
}

/// 计算数据的 MD5 十六进制摘要。
fn md5_hex(data: &[u8]) -> String {
    let mut hasher = Md5::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

fn read_clipboard_text() -> Result<String, String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("Clipboard unavailable: {e}"))?;
    clipboard
        .get_text()
        .map_err(|e| format!("Clipboard has no text: {e}"))
}

/// 将文本写入系统剪贴板（大内容在 Rust 侧处理，避免 WebView 卡顿）。
fn write_clipboard_text(text: &str) -> Result<(), String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("Clipboard unavailable: {e}"))?;
    clipboard
        .set_text(text.to_string())
        .map_err(|e| format!("Failed to copy to clipboard: {e}"))
}

/// 单条编码结果：展示名、临时路径、源体积与 Base64 体积。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct EncodeItem {
    name: String,
    temp_path: String,
    source_size_label: String,
    b64_size_label: String,
}

/// 编码结果：消息 + 可复制条目列表。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct EncodeResult {
    message: String,
    items: Vec<EncodeItem>,
}

/// 路径体积信息，供选中列表展示。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PathSizeInfo {
    path: String,
    size_label: String,
}

/// 带输出路径的操作结果（拆分 / 合并 / 解码）。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PathResult {
    message: String,
    outputs: Vec<String>,
}

/// 编码临时目录：存放待复制的 Base64 文本，不污染源文件旁。
fn encode_temp_dir() -> PathBuf {
    std::env::temp_dir().join("file-tools-encode")
}

/// 为本轮编码创建独立批次目录，避免并行调用互相覆盖。
fn new_encode_batch_dir() -> Result<PathBuf, String> {
    let root = encode_temp_dir();
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let batch = root.join(format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&batch).map_err(|e| e.to_string())?;
    Ok(batch)
}

/// 将目录打成 zip（条目以「目录名/…」为根），写入指定路径。
fn zip_directory(src_dir: &Path, zip_path: &Path) -> Result<(), String> {
    if !src_dir.is_dir() {
        return Err(format!("{} is not a directory.", src_dir.display()));
    }
    let folder_name = src_dir
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("Invalid directory name: {}", src_dir.display()))?;

    let file = File::create(zip_path).map_err(|e| format!("{}: {e}", zip_path.display()))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    // 先写入目录根条目，保证空文件夹也能打出有效 zip
    zip.add_directory(format!("{folder_name}/"), options)
        .map_err(|e| format!("zip {}: {e}", src_dir.display()))?;

    for entry in WalkDir::new(src_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path == src_dir {
            continue;
        }
        let rel = path
            .strip_prefix(src_dir)
            .map_err(|e| format!("strip prefix: {e}"))?;
        let name_in_zip = Path::new(folder_name).join(rel);
        // zip 规范使用正斜杠路径
        let name_str = name_in_zip
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");

        if path.is_dir() {
            let dir_name = if name_str.ends_with('/') {
                name_str
            } else {
                format!("{name_str}/")
            };
            zip.add_directory(dir_name, options)
                .map_err(|e| format!("zip dir {}: {e}", path.display()))?;
        } else if path.is_file() {
            zip.start_file(name_str, options)
                .map_err(|e| format!("zip file {}: {e}", path.display()))?;
            let mut f = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
            std::io::copy(&mut f, &mut zip).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }

    zip.finish()
        .map_err(|e| format!("finalize zip {}: {e}", zip_path.display()))?;
    Ok(())
}

/// 统计文件字节数，或目录内全部文件字节总和。
fn path_byte_size(path: &Path) -> Result<u64, String> {
    if path.is_file() {
        Ok(fs::metadata(path)
            .map_err(|e| format!("{}: {e}", path.display()))?
            .len())
    } else if path.is_dir() {
        let mut total = 0u64;
        for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                total += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
        Ok(total)
    } else {
        Err(format!("Not a file or directory: {}", path.display()))
    }
}

/// 批量查询路径体积，供前端选中列表展示。
#[tauri::command]
async fn path_sizes(paths: Vec<String>) -> Result<Vec<PathSizeInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        paths
            .into_iter()
            .map(|path| {
                let size = path_byte_size(Path::new(&path))?;
                Ok(PathSizeInfo {
                    path,
                    size_label: fsize(size),
                })
            })
            .collect()
    })
    .await
    .map_err(|e| format!("path_sizes task failed: {e}"))?
}

/// 准备待编码字节：普通文件直接读取；目录先压缩为 zip（临时文件，编码后删除）。
fn prepare_encode_payload(path: &Path) -> Result<(String, Vec<u8>), String> {
    if path.is_dir() {
        let folder_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| format!("Invalid directory name: {}", path.display()))?;
        let zip_name = format!("{folder_name}.zip");
        let tmp = std::env::temp_dir().join(format!(
            "file-tools-zip-{}-{}.zip",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        ));
        zip_directory(path, &tmp)?;
        let data = fs::read(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
        let _ = fs::remove_file(&tmp);
        Ok((zip_name, data))
    } else if path.is_file() {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
        let data = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok((name, data))
    } else {
        Err(format!("Not a file or directory: {}", path.display()))
    }
}

/// 将若干文件（或文件夹）编码到临时文本，供前端 Copy；文件夹会先打成 zip 再编码。
fn encode_files_sync(paths: Vec<String>) -> Result<EncodeResult, String> {
    if paths.is_empty() {
        return Err("Select at least one file or folder.".into());
    }
    let outdir = new_encode_batch_dir()?;

    let total = paths.len();
    let mut items = Vec::new();

    for (idx, fi) in paths.iter().enumerate() {
        let path = Path::new(fi);
        let source_bytes = path_byte_size(path)?;
        let (name, data) = prepare_encode_payload(path)?;
        let digest = md5_hex(&data);
        let b64 = B64.encode(&data);

        let out = outdir.join(format!("encode-{}.txt", idx + 1));
        let content = format!("{name}\n{digest}\n{b64}");
        fs::write(&out, content).map_err(|e| format!("{}: {e}", out.display()))?;
        items.push(EncodeItem {
            name,
            temp_path: out.display().to_string(),
            source_size_label: fsize(source_bytes),
            b64_size_label: fsize(b64.len() as u64),
        });
    }

    let message = if items.len() == 1 {
        let it = &items[0];
        format!(
            "Done: 1/{total} encoded · {} · src {} · Base64 {}",
            it.name, it.source_size_label, it.b64_size_label
        )
    } else {
        format!(
            "Done: {}/{total} encoded · use Copy for each",
            items.len()
        )
    };

    Ok(EncodeResult { message, items })
}

#[tauri::command]
async fn encode_files(paths: Vec<String>) -> Result<EncodeResult, String> {
    tauri::async_runtime::spawn_blocking(move || encode_files_sync(paths))
        .await
        .map_err(|e| format!("encode task failed: {e}"))?
}

/// 将文本文件内容复制到剪贴板（Rust 侧读写，支持大文件）。
fn copy_text_file_sync(path: String) -> Result<String, String> {
    let text = fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    write_clipboard_text(&text)?;
    // 优先用编码文本第一行（原始文件名）作为提示
    let label = text
        .lines()
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            Path::new(&path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("output")
                .to_string()
        });
    Ok(format!("Copied {label} to clipboard"))
}

#[tauri::command]
async fn copy_text_file(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || copy_text_file_sync(path))
        .await
        .map_err(|e| format!("copy task failed: {e}"))?
}

/// 清理编码产生的临时文件。
#[tauri::command]
fn clear_encode_temp() -> Result<(), String> {
    let dir = encode_temp_dir();
    let _ = fs::remove_dir_all(&dir);
    Ok(())
}

/// 解析「文件名 / MD5 / Base64」文本，返回 (建议文件名, 原始字节)。
/// 大文本时避免 lines().collect()，只按前两个换行切分头部。
fn parse_decode_text(text: &str) -> Result<(String, Vec<u8>), String> {
    let (name, expect_md5, b64_raw) = split_decode_parts(text)?;
    if name.is_empty() {
        return Err("Filename on line 1 is empty.".into());
    }
    if expect_md5.len() != 32 || !expect_md5.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("Invalid MD5 on line 2.".into());
    }

    // 仅在含空白时才过滤，避免对超大 Base64 做无意义的整串复制
    let data = if b64_raw.bytes().any(|b| b.is_ascii_whitespace()) {
        let compact: String = b64_raw.chars().filter(|c| !c.is_whitespace()).collect();
        B64.decode(compact.as_bytes())
    } else {
        B64.decode(b64_raw.trim().as_bytes())
    }
    .map_err(|e| format!("Base64 decode failed: {e}"))?;

    let actual_md5 = md5_hex(&data);
    if !expect_md5.eq_ignore_ascii_case(&actual_md5) {
        return Err(format!(
            "MD5 mismatch! Expected={expect_md5} Actual={actual_md5}"
        ));
    }

    let out_name = Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(name)
        .to_string();
    Ok((out_name, data))
}

/// 切出 name / md5 / base64 三段，不把全文拆成行向量。
fn split_decode_parts(text: &str) -> Result<(&str, &str, &str), String> {
    let i = text
        .find('\n')
        .ok_or_else(|| "Need at least 3 lines (name, MD5, base64).".to_string())?;
    let name = text[..i].trim_end_matches('\r').trim();
    let rest = &text[i + 1..];
    let j = rest
        .find('\n')
        .ok_or_else(|| "Need at least 3 lines (name, MD5, base64).".to_string())?;
    let md5 = rest[..j].trim_end_matches('\r').trim();
    let b64 = &rest[j + 1..];
    Ok((name, md5, b64))
}

/// 仅解析头部，供粘贴摘要展示（不做 Base64 解码）。
fn peek_decode_header(text: &str) -> Result<(String, String), String> {
    let (name, md5, _) = split_decode_parts(text)?;
    if name.is_empty() {
        return Err("Filename on line 1 is empty.".into());
    }
    if md5.len() != 32 || !md5.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("Invalid MD5 on line 2.".into());
    }
    let out_name = Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(name)
        .to_string();
    Ok((out_name, md5.to_string()))
}

/// 剪贴板粘贴摘要：正文落盘到临时文件，前端只拿元数据。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PasteIngest {
    temp_path: String,
    name: String,
    md5: String,
    chars: u64,
    size_label: String,
}

fn ingest_clipboard_b64_sync() -> Result<PasteIngest, String> {
    let text = read_clipboard_text()?;
    if text.trim().is_empty() {
        return Err("Clipboard is empty.".into());
    }
    let (name, md5) = peek_decode_header(&text)?;
    let chars = text.len() as u64;
    let dir = std::env::temp_dir().join("file-tools-paste");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let temp_path = dir.join(format!(
        "paste-{}.txt",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    fs::write(&temp_path, text.as_bytes()).map_err(|e| e.to_string())?;
    Ok(PasteIngest {
        temp_path: temp_path.display().to_string(),
        name,
        md5,
        chars,
        size_label: fsize(chars),
    })
}

/// 从系统剪贴板读取 Base64 文本并落到临时文件（阻塞工作放到后台线程，避免卡住 UI）。
#[tauri::command]
async fn ingest_clipboard_b64() -> Result<PasteIngest, String> {
    tauri::async_runtime::spawn_blocking(ingest_clipboard_b64_sync)
        .await
        .map_err(|e| format!("ingest task failed: {e}"))?
}

/// 删除粘贴产生的临时文件。
#[tauri::command]
fn clear_paste_temp(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    // 只允许清理本应用临时目录内的文件
    let base = std::env::temp_dir().join("file-tools-paste");
    if p.starts_with(&base) {
        let _ = fs::remove_file(p);
    }
    Ok(())
}

/// 解析 base64 文本文件并还原原始文件，校验 MD5（测试与内部复用）。
fn decode_files_sync(paths: Vec<String>) -> Result<PathResult, String> {
    if paths.is_empty() {
        return Err("Select at least one file.".into());
    }
    let outdir = Path::new(&paths[0])
        .parent()
        .ok_or_else(|| "Invalid input path.".to_string())?;

    let total = paths.len();
    let mut outputs = Vec::new();
    let mut ok = 0usize;

    for fi in &paths {
        let text = fs::read_to_string(fi).map_err(|e| {
            format!(
                "{}: {e}",
                Path::new(fi)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or(fi)
            )
        })?;
        let (out_name, data) =
            parse_decode_text(&text).map_err(|e| format!("{}: {e}", Path::new(fi).display()))?;
        let out = outdir.join(&out_name);
        fs::write(&out, &data).map_err(|e| format!("{}: {e}", out.display()))?;
        outputs.push(out.display().to_string());
        ok += 1;
    }

    let message = if outputs.len() == 1 {
        format!("Done: {ok}/{total} file restored")
    } else {
        format!("Done: {ok}/{total} files restored")
    };

    Ok(PathResult { message, outputs })
}

/// 将粘贴临时文件解码到用户选定的输出路径。
#[tauri::command]
async fn decode_paste_temp(temp_path: String, out_path: String) -> Result<PathResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let text = fs::read_to_string(&temp_path).map_err(|e| e.to_string())?;
        let (_name, data) = parse_decode_text(&text)?;
        let out = Path::new(&out_path);
        if let Some(parent) = out.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
        }
        fs::write(out, &data).map_err(|e| format!("{}: {e}", out.display()))?;
        let _ = fs::remove_file(&temp_path);
        let out_path = out.display().to_string();
        Ok(PathResult {
            message: "Done: 1/1 file restored".into(),
            outputs: vec![out_path],
        })
    })
    .await
    .map_err(|e| format!("decode task failed: {e}"))?
}

/// 按指定块大小拆分文件，生成 .0001 / .0002 … 分片。
#[tauri::command]
fn split_file(path: String, size: u64, unit: String) -> Result<PathResult, String> {
    if size < 1 {
        return Err("Size must be a positive integer.".into());
    }
    let chunk = match unit.as_str() {
        "KB" => size * 1024,
        "MB" => size * 1_048_576,
        "Bytes" => size,
        _ => return Err("Unit must be MB, KB, or Bytes.".into()),
    };

    let fi = Path::new(&path);
    if !fi.is_file() {
        return Err("Select a valid file.".into());
    }

    let mut file = File::open(fi).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; chunk as usize];
    let mut outputs = Vec::new();
    let mut idx = 0u32;

    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        idx += 1;
        let part = format!("{}.{:04}", path, idx);
        let mut out = File::create(&part).map_err(|e| e.to_string())?;
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        outputs.push(part);
    }

    let name = fi.file_name().and_then(|s| s.to_str()).unwrap_or(&path);
    let message = format!("{name}  ->  {idx} part(s)  @ {}", fsize(chunk));
    Ok(PathResult { message, outputs })
}

/// 根据首个分片路径自动发现全部 .NNNN 分片并合并，合并后删除分片。
#[tauri::command]
fn merge_files(first_part: String) -> Result<PathResult, String> {
    let p1 = Path::new(&first_part);
    if !p1.is_file() {
        return Err("Select a valid part file.".into());
    }

    let name = p1
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "Invalid part file name.".to_string())?;

    // 要求文件名以 .NNNN 数字后缀结尾
    let (base_name, _) = name
        .rsplit_once('.')
        .filter(|(_, suf)| suf.len() == 4 && suf.chars().all(|c| c.is_ascii_digit()))
        .ok_or_else(|| "Part file must end with .NNNN".to_string())?;

    let parent = p1.parent().unwrap_or_else(|| Path::new("."));
    let base = parent.join(base_name);
    let fout = base.clone();

    let mut parts = Vec::new();
    let mut i = 1u32;
    loop {
        let part = PathBuf::from(format!("{}.{:04}", base.display(), i));
        if !part.is_file() {
            break;
        }
        parts.push(part);
        i += 1;
    }

    if parts.is_empty() {
        return Err(format!("No parts found for: {}", base.display()));
    }

    let mut total = 0u64;
    {
        let mut out = File::create(&fout).map_err(|e| e.to_string())?;
        for part in &parts {
            let data = fs::read(part).map_err(|e| e.to_string())?;
            total += data.len() as u64;
            out.write_all(&data).map_err(|e| e.to_string())?;
        }
    }

    for part in &parts {
        let _ = fs::remove_file(part);
    }

    let out_path = fout.display().to_string();
    Ok(PathResult {
        message: format!(
            "{} part(s)  ->  {}  ({})",
            parts.len(),
            out_path,
            fsize(total)
        ),
        outputs: vec![out_path],
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            encode_files,
            path_sizes,
            copy_text_file,
            clear_encode_temp,
            ingest_clipboard_b64,
            clear_paste_temp,
            decode_paste_temp,
            split_file,
            merge_files
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;

    #[test]
    fn encode_decode_roundtrip() {
        let dir = temp_dir().join("filetools_tauri_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let src = dir.join("test.bin");
        let data = b"Hello World! ".repeat(500);
        fs::write(&src, &data).unwrap();

        let result = encode_files_sync(vec![src.display().to_string()]).unwrap();
        let b64 = PathBuf::from(&result.items[0].temp_path);
        assert!(b64.is_file());
        assert_eq!(result.items[0].name, "test.bin");
        assert!(!result.items[0].source_size_label.is_empty());
        assert!(!result.items[0].b64_size_label.is_empty());

        // 解码写到临时目录旁；先挪开源文件避免路径混淆
        fs::rename(&src, dir.join("test.bin.bak")).unwrap();
        decode_files_sync(vec![b64.display().to_string()]).unwrap();
        let restored = fs::read(b64.parent().unwrap().join("test.bin")).unwrap();
        assert_eq!(restored, data.as_slice());

        let _ = fs::remove_dir_all(&dir);
        if let Some(batch) = b64.parent() {
            let _ = fs::remove_dir_all(batch);
        }
    }

    #[test]
    fn encode_folder_as_zip() {
        let dir = temp_dir().join("filetools_tauri_folder");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let folder = dir.join("payload");
        fs::create_dir_all(folder.join("nested")).unwrap();
        fs::write(folder.join("a.txt"), b"alpha").unwrap();
        fs::write(folder.join("nested/b.txt"), b"beta").unwrap();

        let result = encode_files_sync(vec![folder.display().to_string()]).unwrap();
        assert_eq!(result.items[0].name, "payload.zip");
        let text = fs::read_to_string(&result.items[0].temp_path).unwrap();
        let (name, bytes) = parse_decode_text(&text).unwrap();
        assert_eq!(name, "payload.zip");
        assert!(!bytes.is_empty());

        // 校验 zip 内包含预期条目
        let cursor = std::io::Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor).unwrap();
        let mut names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        names.sort();
        assert!(names.iter().any(|n| n.contains("a.txt")));
        assert!(names.iter().any(|n| n.contains("b.txt")));

        let _ = fs::remove_dir_all(&dir);
        // 只清本批临时文件，避免并行测试误删其它批次
        if let Some(batch) = Path::new(&result.items[0].temp_path).parent() {
            let _ = fs::remove_dir_all(batch);
        }
    }

    #[test]
    fn split_merge_roundtrip() {
        let dir = temp_dir().join("filetools_tauri_split");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let src = dir.join("blob.bin");
        let data = b"0123456789".repeat(300);
        fs::write(&src, &data).unwrap();

        let split = split_file(src.display().to_string(), 2048, "Bytes".into()).unwrap();
        assert!(!split.outputs.is_empty());
        assert!(PathBuf::from(&split.outputs[0]).is_file());

        // 合并会删除分片并写回同路径，先删原文件模拟仅有分片的场景
        fs::remove_file(&src).unwrap();
        let merged_result = merge_files(format!("{}.0001", src.display())).unwrap();
        assert_eq!(merged_result.outputs[0], src.display().to_string());
        let merged = fs::read(&src).unwrap();
        assert_eq!(merged, data.as_slice());
        assert!(!PathBuf::from(format!("{}.0001", src.display())).exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn decode_pasted_text() {
        let dir = temp_dir().join("filetools_tauri_paste");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let data = b"paste me";
        let digest = md5_hex(data);
        let b64 = B64.encode(data);
        let content = format!("hello.bin\n{digest}\n{b64}");
        let (name, bytes) = parse_decode_text(&content).unwrap();
        assert_eq!(name, "hello.bin");
        assert_eq!(bytes, data);

        let _ = fs::remove_dir_all(&dir);
    }
}
