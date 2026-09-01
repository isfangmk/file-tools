//! File Tools 后端：Base64 编解码、文件拆分与合并。

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use md5::{Digest, Md5};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

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

/// 编码结果：消息 + 输出文件路径列表（供前端展示复制按钮）。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct EncodeResult {
    message: String,
    outputs: Vec<String>,
}

/// 将若干文件编码为 base64-N.txt。
fn encode_files_sync(paths: Vec<String>) -> Result<EncodeResult, String> {
    if paths.is_empty() {
        return Err("Select at least one file.".into());
    }
    let outdir = Path::new(&paths[0])
        .parent()
        .ok_or_else(|| "Invalid input path.".to_string())?;

    let total = paths.len();
    let mut outputs = Vec::new();
    let mut ok = 0usize;

    for (idx, fi) in paths.iter().enumerate() {
        let path = Path::new(fi);
        let data = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
        let digest = md5_hex(&data);
        let b64 = B64.encode(&data);

        let out = outdir.join(format!("base64-{}.txt", idx + 1));
        let content = format!("{name}\n{digest}\n{b64}");
        fs::write(&out, content).map_err(|e| format!("{}: {e}", out.display()))?;
        outputs.push(out.display().to_string());
        ok += 1;
    }

    let message = if outputs.len() == 1 {
        format!("Done: {ok}/{total} file encoded -> {}", outputs[0])
    } else {
        format!(
            "Done: {ok}/{total} files encoded:\n{}",
            outputs.join("\n")
        )
    };

    Ok(EncodeResult { message, outputs })
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
    let label = Path::new(&path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    Ok(format!("Copied {label} to clipboard"))
}

#[tauri::command]
async fn copy_text_file(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || copy_text_file_sync(path))
        .await
        .map_err(|e| format!("copy task failed: {e}"))?
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

/// 解析 base64 文本文件并还原原始文件，校验 MD5。
#[tauri::command]
async fn decode_files(paths: Vec<String>) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || decode_files_sync(paths))
        .await
        .map_err(|e| format!("decode task failed: {e}"))?
}

fn decode_files_sync(paths: Vec<String>) -> Result<String, String> {
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

    if outputs.len() == 1 {
        Ok(format!("Done: {ok}/{total} file restored -> {}", outputs[0]))
    } else {
        Ok(format!(
            "Done: {ok}/{total} files restored:\n{}",
            outputs.join("\n")
        ))
    }
}

/// 将粘贴临时文件解码到用户选定的输出路径。
#[tauri::command]
async fn decode_paste_temp(temp_path: String, out_path: String) -> Result<String, String> {
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
        Ok(format!("Done: 1/1 file restored -> {}", out.display()))
    })
    .await
    .map_err(|e| format!("decode task failed: {e}"))?
}

/// 按指定块大小拆分文件，生成 .0001 / .0002 … 分片。
#[tauri::command]
fn split_file(path: String, size: u64, unit: String) -> Result<String, String> {
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
    }

    let name = fi.file_name().and_then(|s| s.to_str()).unwrap_or(&path);
    Ok(format!(
        "{name}  ->  {idx} part(s)  @ {}",
        fsize(chunk)
    ))
}

/// 根据首个分片路径自动发现全部 .NNNN 分片并合并，合并后删除分片。
#[tauri::command]
fn merge_files(first_part: String) -> Result<String, String> {
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

    Ok(format!(
        "{} part(s)  ->  {}  ({})",
        parts.len(),
        fout.display(),
        fsize(total)
    ))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            encode_files,
            copy_text_file,
            decode_files,
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
        let b64 = PathBuf::from(&result.outputs[0]);
        assert!(b64.is_file());

        // 解码会写出同名文件，先挪开原文件避免覆盖干扰断言
        fs::rename(&src, dir.join("test.bin.bak")).unwrap();
        decode_files_sync(vec![b64.display().to_string()]).unwrap();
        let restored = fs::read(dir.join("test.bin")).unwrap();
        assert_eq!(restored, data.as_slice());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn split_merge_roundtrip() {
        let dir = temp_dir().join("filetools_tauri_split");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let src = dir.join("blob.bin");
        let data = b"0123456789".repeat(300);
        fs::write(&src, &data).unwrap();

        split_file(src.display().to_string(), 2048, "Bytes".into()).unwrap();
        assert!(PathBuf::from(format!("{}.0001", src.display())).is_file());

        // 合并会删除分片并写回同路径，先删原文件模拟仅有分片的场景
        fs::remove_file(&src).unwrap();
        merge_files(format!("{}.0001", src.display())).unwrap();
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
