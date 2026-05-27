use crate::services::canon::extract::SourceKind;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Canon document kind. Drives prompt-injection grouping and retrieval bias.
/// (A faction-notes file is treated differently than a magic-rules file.)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum CanonKind {
    Character,
    Location,
    Faction,
    Magic,
    History,
    Cosmology,
    Timeline,
    #[default]
    Lore,
    PlotNotes,
    DmNotes,
    Other,
}

/// Sensitivity tier honored by retrieval filters.
///
/// `Public` — default, may be sent to cloud LLMs.
/// `Spoiler` — book-late reveals; excluded from early-chapter context unless
///   the user explicitly opts in for a scene.
/// `DoNotSend` — never transmitted to any cloud provider, regardless.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChunkSensitivity {
    #[default]
    Public,
    Spoiler,
    DoNotSend,
}

/// One per-project rule that maps a vault-relative path pattern to a
/// sensitivity. See `services::canon::rules` for matching semantics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VaultRule {
    pub pattern: String,
    pub sensitivity: ChunkSensitivity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonDocument {
    pub id: String,
    pub project_id: String,
    pub source_path: String,
    pub kind: CanonKind,
    pub source_kind: SourceKind,
    pub ingested_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub chunk_count: u32,
    pub byte_size: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CanonChunk {
    pub id: String,
    pub doc_id: String,
    pub project_id: String,
    pub index: u32,
    pub offset: u32,
    pub text: String,
    pub headings: Vec<String>,
    pub word_count: u32,
    pub sensitivity: ChunkSensitivity,
    /// Absolute path of the source file this chunk came from. Carried on
    /// the chunk (in addition to the document record) so vault rules can
    /// be re-applied retroactively without re-ingesting.
    /// Defaulted in serde for backward compat with v0.2 indices written
    /// before this field existed.
    pub source_path: String,
}

/// What retrieval returns: the chunk itself plus its similarity score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkRef {
    pub id: String,
    pub doc_id: String,
    pub project_id: String,
    pub index: u32,
    pub offset: u32,
    pub text: String,
    pub headings: Vec<String>,
    pub word_count: u32,
    pub sensitivity: ChunkSensitivity,
    /// Cosine similarity, 0..=1. Higher is better.
    pub score: f32,
}

impl ChunkRef {
    pub fn from_chunk(c: &CanonChunk, score: f32) -> Self {
        Self {
            id: c.id.clone(),
            doc_id: c.doc_id.clone(),
            project_id: c.project_id.clone(),
            index: c.index,
            offset: c.offset,
            text: c.text.clone(),
            headings: c.headings.clone(),
            word_count: c.word_count,
            sensitivity: c.sensitivity,
            score,
        }
    }
}
