use crate::models::canon::{ChunkSensitivity, VaultRule};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A Project corresponds to one book.
///
/// All project content lives under
/// `<data_dir>/projects/<id>/` — see `services::storage::project::layout`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub manuscript_word_count: u64,
    /// 0..15 — how many of the 15 Save-the-Cat beats have been touched.
    pub beat_progress: u8,
    /// Absolute path to an external Obsidian vault directory to ingest from.
    /// `None` means no vault is linked. Per-project so different books can
    /// reference different worldbuilding corpora.
    pub vault_path: Option<String>,
    /// When true, ingestion runs automatically as files in `vault_path` are
    /// modified (debounced). When false, ingestion stays manual.
    pub vault_auto_watch: bool,
    /// Per-project folder/path rules that map vault paths to a sensitivity
    /// tier. Consulted on every ingest (auto or manual) when the caller
    /// doesn't explicitly override.
    pub vault_rules: Vec<VaultRule>,
    /// Sensitivity used for any vault file not matched by a rule and not
    /// tagged via frontmatter. Defaults to `Public` (backward-compatible
    /// with v0.2 projects).
    pub vault_default_sensitivity: ChunkSensitivity,
}

impl Default for Project {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            id: String::new(),
            name: String::new(),
            created_at: now,
            updated_at: now,
            manuscript_word_count: 0,
            beat_progress: 0,
            vault_path: None,
            vault_auto_watch: false,
            vault_rules: Vec::new(),
            vault_default_sensitivity: ChunkSensitivity::Public,
        }
    }
}

impl Project {
    pub fn new(name: String) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            created_at: now,
            updated_at: now,
            manuscript_word_count: 0,
            beat_progress: 0,
            vault_path: None,
            vault_auto_watch: false,
            vault_rules: Vec::new(),
            vault_default_sensitivity: ChunkSensitivity::Public,
        }
    }
}

/// Partial update for `Project`. All fields optional; missing fields leave
/// the existing value untouched.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ProjectPatch {
    pub name: Option<String>,
    /// Use `Some(None)` to clear the linked vault, `Some(Some(path))` to set it.
    pub vault_path: Option<Option<String>>,
    pub vault_auto_watch: Option<bool>,
    /// Wholesale replace the rules list. Pass `Some(Vec::new())` to clear.
    pub vault_rules: Option<Vec<VaultRule>>,
    pub vault_default_sensitivity: Option<ChunkSensitivity>,
}

impl ProjectPatch {
    pub fn apply(self, p: &mut Project) {
        if let Some(name) = self.name {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                p.name = trimmed.to_string();
            }
        }
        if let Some(v) = self.vault_path {
            p.vault_path = v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        }
        if let Some(v) = self.vault_auto_watch {
            p.vault_auto_watch = v;
        }
        if let Some(v) = self.vault_rules {
            p.vault_rules = v;
        }
        if let Some(v) = self.vault_default_sensitivity {
            p.vault_default_sensitivity = v;
        }
        p.updated_at = Utc::now();
    }
}
