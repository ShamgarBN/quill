//! Local-only audit log for cloud LLM calls.
//!
//! Append-only JSON-lines file at `<data_dir>/audit.log`. Every cloud
//! request produces one line. Inspectable via `tail -f`.
//!
//! IMPORTANT: We log *categories* of content sent (e.g. "scene_card",
//! "canon_top5") — never the content itself. The audit log is for the
//! user's privacy review, not for debugging prompts.

use crate::error::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IncludedCategory {
    SceneCard,
    BeatDescription,
    /// Free-form POV string declared on the scene (e.g. "Kaelan, 3rd-limited").
    CharacterPov,
    /// Structured Character Bible entry matched by POV name + aliases.
    /// Distinct from `CharacterPov` because the Bible carries motivation,
    /// voice, arc, etc. that the scene's POV string doesn't.
    CharacterBibleEntry,
    /// Setting-kind canon chunk(s) matched by the scene's `setting` value.
    SettingCanon,
    /// Idea Park entries tagged for the active beat/scene/POV.
    IdeaPark,
    RecentParagraphs,
    CanonTopK,
    ReferencePins,
    UserPrompt,
    SystemPrompt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: DateTime<Utc>,
    pub provider: String,
    pub model: String,
    pub operation: String,
    pub project_id: Option<String>,
    pub scene_id: Option<String>,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub included: Vec<IncludedCategory>,
    pub success: bool,
    pub error: Option<String>,
}

pub struct AuditLog {
    path: PathBuf,
    inner: Mutex<()>,
}

impl AuditLog {
    pub fn open(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        Ok(Self {
            path: data_dir.join("audit.log"),
            inner: Mutex::new(()),
        })
    }

    pub fn append(&self, entry: &AuditEntry) -> Result<()> {
        let line = serde_json::to_string(entry)?;
        let _g = self.inner.lock().expect("audit lock poisoned");
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(f, "{line}")?;
        f.sync_all()?;
        Ok(())
    }

    pub fn tail(&self, limit: usize) -> Result<Vec<AuditEntry>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let _g = self.inner.lock().expect("audit lock poisoned");
        let bytes = std::fs::read(&self.path)?;
        let s = String::from_utf8_lossy(&bytes);
        let lines: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
        let start = lines.len().saturating_sub(limit);
        let mut out = Vec::new();
        for line in &lines[start..] {
            match serde_json::from_str::<AuditEntry>(line) {
                Ok(e) => out.push(e),
                Err(e) => {
                    tracing::warn!(?line, error = %e, "skipping malformed audit line");
                }
            }
        }
        Ok(out)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(op: &str) -> AuditEntry {
        AuditEntry {
            timestamp: Utc::now(),
            provider: "mock".into(),
            model: "mock".into(),
            operation: op.into(),
            project_id: Some("p1".into()),
            scene_id: None,
            tokens_in: 100,
            tokens_out: 200,
            included: vec![IncludedCategory::SceneCard],
            success: true,
            error: None,
        }
    }

    #[test]
    fn append_and_tail() {
        let dir = tempfile::tempdir().unwrap();
        let log = AuditLog::open(dir.path()).unwrap();
        for op in ["scene_draft", "paragraph_cowrite", "critique"] {
            log.append(&entry(op)).unwrap();
        }
        let tail = log.tail(2).unwrap();
        assert_eq!(tail.len(), 2);
        assert_eq!(tail[0].operation, "paragraph_cowrite");
        assert_eq!(tail[1].operation, "critique");
    }

    #[test]
    fn skips_malformed_lines() {
        let dir = tempfile::tempdir().unwrap();
        let log = AuditLog::open(dir.path()).unwrap();
        log.append(&entry("ok")).unwrap();
        // Inject garbage
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(log.path())
            .unwrap();
        writeln!(f, "not json").unwrap();
        log.append(&entry("after_garbage")).unwrap();
        let tail = log.tail(10).unwrap();
        assert_eq!(tail.len(), 2);
    }
}
