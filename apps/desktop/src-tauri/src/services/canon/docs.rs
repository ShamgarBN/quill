//! Per-document metadata store. Lives at `<project>/canon/docs.json`.
//!
//! What it persists: the per-doc extraction toggle + last-extracted
//! timestamp. CanonChunk records carry their own kind/sensitivity, but
//! that data is per-chunk and lives in the vector index. This is the
//! one place where doc-level state goes.
//!
//! Lookup is defensive — any doc_id not present in the file returns
//! `DocMeta::defaults_for(doc_id)` (extraction enabled, never extracted).

use crate::error::Result;
use crate::models::canon::DocMeta;
use crate::services::storage::{self, ProjectStore};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct DocsFile {
    #[serde(default)]
    pub docs: Vec<DocMeta>,
}

pub struct DocMetaStore<'a> {
    projects: &'a ProjectStore,
}

impl<'a> DocMetaStore<'a> {
    pub fn new(projects: &'a ProjectStore) -> Self {
        Self { projects }
    }

    fn path(&self, project_id: &str) -> Result<PathBuf> {
        let dir = self.projects.root_dir(project_id)?.join("canon");
        std::fs::create_dir_all(&dir)?;
        Ok(dir.join("docs.json"))
    }

    pub fn list(&self, project_id: &str) -> Result<Vec<DocMeta>> {
        let p = self.path(project_id)?;
        let f: DocsFile = storage::atomic_read_json_or_default(&p)?;
        Ok(f.docs)
    }

    pub fn get(&self, project_id: &str, doc_id: &str) -> Result<DocMeta> {
        let docs = self.list(project_id)?;
        Ok(docs
            .into_iter()
            .find(|d| d.doc_id == doc_id)
            .unwrap_or_else(|| DocMeta::defaults_for(doc_id)))
    }

    fn save(&self, project_id: &str, docs: &[DocMeta]) -> Result<()> {
        let p = self.path(project_id)?;
        storage::atomic_write_json(
            &p,
            &DocsFile {
                docs: docs.to_vec(),
            },
        )
    }

    /// Set the extraction toggle for one doc. Creates the entry if it
    /// doesn't exist yet.
    pub fn set_extraction_enabled(
        &self,
        project_id: &str,
        doc_id: &str,
        enabled: bool,
    ) -> Result<DocMeta> {
        let mut docs = self.list(project_id)?;
        if let Some(m) = docs.iter_mut().find(|d| d.doc_id == doc_id) {
            m.extraction_enabled = enabled;
        } else {
            let mut fresh = DocMeta::defaults_for(doc_id);
            fresh.extraction_enabled = enabled;
            docs.push(fresh);
        }
        self.save(project_id, &docs)?;
        self.get(project_id, doc_id)
    }

    /// Stamp `last_extracted_at = now` for a doc. Creates the entry if
    /// it doesn't exist yet.
    pub fn mark_extracted(&self, project_id: &str, doc_id: &str) -> Result<()> {
        let mut docs = self.list(project_id)?;
        let now = Utc::now();
        if let Some(m) = docs.iter_mut().find(|d| d.doc_id == doc_id) {
            m.last_extracted_at = Some(now);
        } else {
            let mut fresh = DocMeta::defaults_for(doc_id);
            fresh.last_extracted_at = Some(now);
            docs.push(fresh);
        }
        self.save(project_id, &docs)
    }

    /// Drop a doc's entry — used when the doc itself is deleted from
    /// the index, so we don't accumulate phantom entries.
    pub fn forget(&self, project_id: &str, doc_id: &str) -> Result<()> {
        let mut docs = self.list(project_id)?;
        let before = docs.len();
        docs.retain(|d| d.doc_id != doc_id);
        if docs.len() != before {
            self.save(project_id, &docs)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::storage::ProjectStore;

    #[test]
    fn unknown_doc_returns_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let p = projects.create("Demo").unwrap();
        let store = DocMetaStore::new(&projects);
        let m = store.get(&p.id, "doc_nope").unwrap();
        assert_eq!(m.doc_id, "doc_nope");
        assert!(m.extraction_enabled);
        assert!(m.last_extracted_at.is_none());
    }

    #[test]
    fn toggle_extraction_persists() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let p = projects.create("Demo").unwrap();
        let store = DocMetaStore::new(&projects);
        store.set_extraction_enabled(&p.id, "doc_a", false).unwrap();
        assert!(!store.get(&p.id, "doc_a").unwrap().extraction_enabled);
        store.set_extraction_enabled(&p.id, "doc_a", true).unwrap();
        assert!(store.get(&p.id, "doc_a").unwrap().extraction_enabled);
    }

    #[test]
    fn mark_extracted_stamps_timestamp() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let p = projects.create("Demo").unwrap();
        let store = DocMetaStore::new(&projects);
        assert!(store.get(&p.id, "doc_x").unwrap().last_extracted_at.is_none());
        store.mark_extracted(&p.id, "doc_x").unwrap();
        assert!(store.get(&p.id, "doc_x").unwrap().last_extracted_at.is_some());
    }

    #[test]
    fn forget_drops_entry() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let p = projects.create("Demo").unwrap();
        let store = DocMetaStore::new(&projects);
        store.set_extraction_enabled(&p.id, "doc_a", false).unwrap();
        store.forget(&p.id, "doc_a").unwrap();
        // After forget, defaults apply again.
        assert!(store.get(&p.id, "doc_a").unwrap().extraction_enabled);
    }
}
