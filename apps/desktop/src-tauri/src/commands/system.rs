//! Lightweight system-integration commands (Finder reveal, etc.).
//!
//! These are intentionally minimal — anything more elaborate (open with
//! specific app, drag-and-drop integration) should go through the
//! `tauri-plugin-shell` or `tauri-plugin-opener` plugins instead of
//! here.

use crate::error::{QuillError, Result};
use std::path::Path;

/// Reveal `path` in Finder. If it's a directory, the directory itself is
/// shown in a new Finder window. If it's a file, Finder opens the parent
/// directory with the file selected.
#[tauri::command]
pub fn system_reveal_path(path: String) -> Result<()> {
    let p = Path::new(&path);
    if !p.exists() {
        return Err(QuillError::NotFound(format!(
            "cannot reveal: path does not exist ({})",
            p.display()
        )));
    }
    // `open -R <path>` selects the item in Finder; for a directory, this
    // opens its parent with the directory selected. `open <dir>` opens the
    // directory itself — better behavior for project roots.
    let status = if p.is_dir() {
        std::process::Command::new("/usr/bin/open")
            .arg(&path)
            .status()
    } else {
        std::process::Command::new("/usr/bin/open")
            .args(["-R", &path])
            .status()
    };
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(QuillError::Internal(format!("open exited with status {s}"))),
        Err(e) => Err(QuillError::Internal(format!("failed to launch open: {e}"))),
    }
}
