//! Storage service: project filesystem layout, settings file, audit log.
//!
//! Storage discipline:
//! - Manuscript content lives in plain Markdown — never trapped.
//! - Project metadata + app settings live in human-readable JSON.
//! - Atomic writes (write to `.tmp` + rename) for any file we care about.

mod atomic;
mod project;
mod settings;

pub use project::ProjectStore;
pub use settings::SettingsStore;

use crate::error::Result;
use std::path::Path;

/// Re-exported atomic write helper for use by sibling services (e.g. crypto).
pub fn atomic_write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    atomic::write_json(path, value)
}

/// Re-exported atomic byte write helper for non-JSON content (e.g. Markdown).
pub fn atomic_write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    atomic::write_bytes(path, bytes)
}

/// Read JSON from disk; if file missing, return `T::default()`.
pub fn atomic_read_json_or_default<T>(path: &Path) -> Result<T>
where
    T: serde::de::DeserializeOwned + Default,
{
    if path.exists() {
        atomic::read_json(path)
    } else {
        Ok(T::default())
    }
}
