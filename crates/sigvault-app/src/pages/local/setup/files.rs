//! `setup/files.ts` — text file save / open through the native dialogs.
//! The file I/O itself runs on the backend runtime, off the UI thread.
//!
//! GPUI's dialogs take no extension filters, so the TS `*_FILTERS`
//! lists only survive as the suggested file name's extension.

use std::path::PathBuf;

use gpui::{AsyncApp, PathPromptOptions};

use crate::runtime::backend;

fn default_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .map(|home| {
            let downloads = home.join("Downloads");
            if downloads.is_dir() {
                downloads
            } else {
                home
            }
        })
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Returns the path written, or `None` when the user cancelled.
pub async fn save_text_file(
    cx: &mut AsyncApp,
    contents: String,
    default_name: String,
) -> Result<Option<String>, String> {
    let dir = default_dir();
    let rx = cx
        .update(|cx| cx.prompt_for_new_path(&dir, Some(&default_name)))
        .map_err(|e| e.to_string())?;
    let path = match rx.await {
        Ok(Ok(Some(p))) => p,
        Ok(Ok(None)) | Err(_) => return Ok(None),
        Ok(Err(e)) => return Err(e.to_string()),
    };
    let task = cx
        .update(|cx| {
            backend(cx).spawn(async move {
                std::fs::write(&path, contents.as_bytes())
                    .map(|_| path.display().to_string())
                    .map_err(|e| e.to_string())
            })
        })
        .map_err(|e| e.to_string())?;
    match task.await {
        Some(r) => r.map(Some),
        None => Err("Failed to write file".into()),
    }
}

pub struct OpenedFile {
    pub name: String,
    pub contents: String,
}

/// Returns `{ name, contents }`, or `None` when the user cancelled.
pub async fn open_text_file(cx: &mut AsyncApp, title: &str) -> Result<Option<OpenedFile>, String> {
    let rx = cx
        .update(|cx| {
            cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false,
                multiple: false,
                prompt: Some(title.to_string().into()),
            })
        })
        .map_err(|e| e.to_string())?;
    let path = match rx.await {
        Ok(Ok(Some(paths))) => match paths.into_iter().next() {
            Some(p) => p,
            None => return Ok(None),
        },
        Ok(Ok(None)) | Err(_) => return Ok(None),
        Ok(Err(e)) => return Err(e.to_string()),
    };
    let task = cx
        .update(|cx| {
            backend(cx).spawn(async move {
                let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.display().to_string());
                Ok::<_, String>(OpenedFile {
                    name,
                    contents: String::from_utf8_lossy(&bytes).to_string(),
                })
            })
        })
        .map_err(|e| e.to_string())?;
    match task.await {
        Some(r) => r.map(Some),
        None => Err("Failed to read file".into()),
    }
}

pub fn file_safe(name: &str) -> String {
    let kept: String = name
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ' '))
        .collect();
    let cleaned = kept.split_whitespace().collect::<Vec<_>>().join("-");
    if cleaned.is_empty() {
        "wallet".into()
    } else {
        cleaned.to_lowercase()
    }
}

#[cfg(test)]
mod tests {
    use super::file_safe;

    #[test]
    fn file_safe_names() {
        assert_eq!(file_safe("My Savings!"), "my-savings");
        assert_eq!(file_safe("  "), "wallet");
        assert_eq!(file_safe("Key A"), "key-a");
    }
}
