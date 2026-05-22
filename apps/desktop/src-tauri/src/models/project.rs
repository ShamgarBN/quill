use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A Project corresponds to one book.
///
/// All project content lives under
/// `<data_dir>/projects/<id>/` — see `services::storage::project::layout`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub manuscript_word_count: u64,
    /// 0..15 — how many of the 15 Save-the-Cat beats have been touched.
    pub beat_progress: u8,
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
        }
    }
}
