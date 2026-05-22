//! Tauri-managed application state.
//!
//! Constructed once at startup; held by the Tauri runtime and injected into
//! command handlers via `tauri::State`.

use crate::error::Result;
use crate::services::{crypto::SecretStore, storage::ProjectStore, storage::SettingsStore};
use std::path::PathBuf;
use std::sync::Mutex;

pub struct AppState {
    pub data_dir: PathBuf,
    pub projects: ProjectStore,
    pub settings_store: SettingsStore,
    pub settings_lock: Mutex<()>,
    pub secrets: SecretStore,
}

impl AppState {
    pub fn initialize(data_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&data_dir)?;
        let projects = ProjectStore::new(&data_dir);
        let settings_store = SettingsStore::new(&data_dir);
        // Trigger first-run defaults if missing.
        let _ = settings_store.load_or_init()?;
        let secrets = SecretStore::initialize(&data_dir)?;

        Ok(Self {
            data_dir,
            projects,
            settings_store,
            settings_lock: Mutex::new(()),
            secrets,
        })
    }
}
