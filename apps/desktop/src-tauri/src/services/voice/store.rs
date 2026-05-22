//! Reference-pin store: per-project list of voice references the user has
//! pinned. Each pin is a passage of text plus metadata (author, source,
//! weight). Stored as JSON at `<project>/voice/pins.json`.

use crate::error::Result;
use crate::services::storage;
use crate::services::storage::ProjectStore;
use crate::services::voice::extractor::{extract_features, VoiceFeatures};
use crate::services::voice::fingerprint::{build_fingerprint, VoiceFingerprint};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferencePin {
    pub id: String,
    pub project_id: String,
    pub label: String,
    /// Author of origin (e.g. "Christopher Paolini"). Free text — no
    /// licensing claim attached. Pins are user-owned and stored locally.
    pub author: Option<String>,
    pub source: Option<String>,
    pub passage: String,
    /// Relative weight when contributing to the fingerprint centroid.
    /// Defaults to 1.0; user can dial up "this voice cluster matters more."
    pub weight: f32,
    /// Whether this pin is currently active (counted in fingerprint).
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ReferencePin {
    pub fn new(project_id: &str, label: &str, passage: &str) -> Self {
        let now = Utc::now();
        Self {
            id: format!("ref_{}", Uuid::new_v4().simple()),
            project_id: project_id.to_string(),
            label: label.to_string(),
            author: None,
            source: None,
            passage: passage.to_string(),
            weight: 1.0,
            enabled: true,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PinFile {
    pub pins: Vec<ReferencePin>,
}

pub struct ReferencePinStore<'a> {
    projects: &'a ProjectStore,
}

impl<'a> ReferencePinStore<'a> {
    pub fn new(projects: &'a ProjectStore) -> Self {
        Self { projects }
    }

    fn path(&self, project_id: &str) -> Result<PathBuf> {
        let dir = self.projects.root_dir(project_id)?.join("voice");
        std::fs::create_dir_all(&dir)?;
        Ok(dir.join("pins.json"))
    }

    pub fn list(&self, project_id: &str) -> Result<Vec<ReferencePin>> {
        let p = self.path(project_id)?;
        if !p.exists() {
            return Ok(Vec::new());
        }
        let f: PinFile = storage::atomic_read_json_or_default(&p)?;
        Ok(f.pins)
    }

    pub fn save(&self, project_id: &str, pins: &[ReferencePin]) -> Result<()> {
        let p = self.path(project_id)?;
        storage::atomic_write_json(
            &p,
            &PinFile {
                pins: pins.to_vec(),
            },
        )
    }

    pub fn create(&self, project_id: &str, label: &str, passage: &str) -> Result<ReferencePin> {
        if passage.trim().is_empty() {
            return Err(crate::error::QuillError::InvalidArgument(
                "reference passage cannot be empty".into(),
            ));
        }
        let mut pins = self.list(project_id)?;
        let pin = ReferencePin::new(project_id, label, passage);
        pins.push(pin.clone());
        self.save(project_id, &pins)?;
        Ok(pin)
    }

    pub fn delete(&self, project_id: &str, id: &str) -> Result<()> {
        let mut pins = self.list(project_id)?;
        pins.retain(|p| p.id != id);
        self.save(project_id, &pins)
    }

    pub fn update<F>(&self, project_id: &str, id: &str, f: F) -> Result<ReferencePin>
    where
        F: FnOnce(&mut ReferencePin),
    {
        let mut pins = self.list(project_id)?;
        let pin = pins
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| crate::error::QuillError::NotFound(format!("pin {id}")))?;
        f(pin);
        pin.updated_at = Utc::now();
        let updated = pin.clone();
        self.save(project_id, &pins)?;
        Ok(updated)
    }

    /// Compute the project's voice fingerprint from all enabled pins, with
    /// per-pin weight applied by replicating the feature contribution.
    pub fn fingerprint(&self, project_id: &str) -> Result<VoiceFingerprint> {
        let pins = self.list(project_id)?;
        let features: Vec<VoiceFeatures> = pins
            .iter()
            .filter(|p| p.enabled)
            .flat_map(|p| {
                let f = extract_features(&p.passage);
                let copies = (p.weight.clamp(0.0, 10.0) * 100.0).round() as usize / 100;
                let copies = copies.max(1);
                std::iter::repeat(f).take(copies).collect::<Vec<_>>()
            })
            .collect();
        Ok(build_fingerprint(&features))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::storage::ProjectStore;

    #[test]
    fn create_list_delete() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let project = projects.create("Demo").unwrap();
        let store = ReferencePinStore::new(&projects);

        assert!(store.list(&project.id).unwrap().is_empty());

        let p1 = store
            .create(&project.id, "Eragon ch1", "He gripped the bow tighter.")
            .unwrap();
        let p2 = store
            .create(
                &project.id,
                "Lightning Thief",
                "Look, I didn't want to be a half-blood.",
            )
            .unwrap();

        let listed = store.list(&project.id).unwrap();
        assert_eq!(listed.len(), 2);

        store.delete(&project.id, &p1.id).unwrap();
        let after = store.list(&project.id).unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].id, p2.id);
    }

    #[test]
    fn fingerprint_uses_only_enabled_pins() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let project = projects.create("Demo").unwrap();
        let store = ReferencePinStore::new(&projects);

        let a = store
            .create(
                &project.id,
                "A",
                "Short. Punchy. Hard. The boy ran into the dark.",
            )
            .unwrap();
        let _b = store
            .create(
                &project.id,
                "B",
                "An elaborately constructed clause unfurls itself in the manner of an heirloom carpet, lavish in its embellishments and unhurried in its progress.",
            )
            .unwrap();

        let fp_all = store.fingerprint(&project.id).unwrap();
        // Disable B → fingerprint should reflect only A
        store
            .update(&project.id, &_b.id, |p| {
                p.enabled = false;
            })
            .unwrap();
        let fp_only_a = store.fingerprint(&project.id).unwrap();

        // mean_sentence_words index = 0; A's mean is small, full set's is larger
        assert!(
            fp_only_a.mean[0] < fp_all.mean[0],
            "disabling B should reduce mean sentence length: {} < {}",
            fp_only_a.mean[0],
            fp_all.mean[0]
        );
        let _ = a;
    }
}
