//! Tauri-managed application state.
//!
//! Constructed once at startup; held by the Tauri runtime and injected into
//! command handlers via `tauri::State`.

use crate::error::Result;
use crate::services::{
    crypto::SecretStore,
    llm::{AuditLog, ProviderRegistry},
    storage::ProjectStore,
    storage::SettingsStore,
    vector::{JsonVectorStore, VectorStore},
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct AppState {
    pub data_dir: PathBuf,
    pub projects: ProjectStore,
    pub settings_store: SettingsStore,
    pub settings_lock: Mutex<()>,
    pub secrets: Arc<SecretStore>,
    pub audit: Arc<AuditLog>,
    pub vectors: Arc<dyn VectorStore>,
    pub providers: ProviderRegistry,
}

impl AppState {
    pub fn initialize(data_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&data_dir)?;
        let projects = ProjectStore::new(&data_dir);
        let settings_store = SettingsStore::new(&data_dir);
        let _ = settings_store.load_or_init()?;

        let secrets = Arc::new(SecretStore::initialize(&data_dir)?);
        let audit = Arc::new(AuditLog::open(&data_dir)?);

        let vector_path = data_dir.join("vectors.json");
        let vectors: Arc<dyn VectorStore> = Arc::new(JsonVectorStore::open(vector_path)?);

        let providers = ProviderRegistry::new(secrets.clone());

        Ok(Self {
            data_dir,
            projects,
            settings_store,
            settings_lock: Mutex::new(()),
            secrets,
            audit,
            vectors,
            providers,
        })
    }
}
