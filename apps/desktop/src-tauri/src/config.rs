//! Runtime configuration: where data lives, what version we are, etc.

use crate::error::{QuillError, Result};
use directories::ProjectDirs;
use std::path::PathBuf;
use tauri::AppHandle;

/// App-info constants surfaced to the UI.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_PHASE: &str = "Phase 0 — Foundation";

/// Resolve the application data directory.
///
/// Order of precedence:
/// 1. `QUILL_DATA_DIR` env var (used by dev runs to keep the dev profile out
///    of the production app-support folder).
/// 2. macOS standard: `~/Library/Application Support/Quill`.
pub fn resolve_data_dir(_app: &AppHandle) -> Result<PathBuf> {
    if let Ok(custom) = std::env::var("QUILL_DATA_DIR") {
        let p = PathBuf::from(custom);
        std::fs::create_dir_all(&p)?;
        return Ok(p);
    }

    let proj = ProjectDirs::from("com", "Shamgar", "Quill").ok_or_else(|| {
        QuillError::Internal("could not resolve project directory on this platform".into())
    })?;
    let dir = proj.data_dir().to_path_buf();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}
