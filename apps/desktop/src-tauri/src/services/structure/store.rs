//! Per-project beat sheet + scene list storage.
//!
//! Files:
//!   <project>/structure/beat_sheet.json
//!   <project>/structure/scenes.json

use crate::error::{QuillError, Result};
use crate::models::structure::{Beat, BeatId, BeatSheet, Scene, SceneList};
use crate::services::storage;
use crate::services::storage::ProjectStore;
use chrono::Utc;
use std::path::PathBuf;

pub struct StructureStore<'a> {
    pub projects: &'a ProjectStore,
}

impl<'a> StructureStore<'a> {
    pub fn new(projects: &'a ProjectStore) -> Self {
        Self { projects }
    }

    fn dir(&self, project_id: &str) -> Result<PathBuf> {
        let root = self.projects.root_dir(project_id)?;
        let dir = root.join("structure");
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    fn beat_sheet_path(&self, project_id: &str) -> Result<PathBuf> {
        Ok(self.dir(project_id)?.join("beat_sheet.json"))
    }

    fn scenes_path(&self, project_id: &str) -> Result<PathBuf> {
        Ok(self.dir(project_id)?.join("scenes.json"))
    }

    // ---------- Beat sheet ----------

    pub fn load_beat_sheet(&self, project_id: &str) -> Result<BeatSheet> {
        let path = self.beat_sheet_path(project_id)?;
        if path.exists() {
            let bytes = std::fs::read(&path)?;
            let mut sheet: BeatSheet = serde_json::from_slice(&bytes)?;
            // Sanity: ensure all 15 beats are present (forward-compat) and in
            // canonical order. If any are missing, append them in the right
            // place; if extras appear, keep them at end.
            ensure_all_beats(&mut sheet);
            Ok(sheet)
        } else {
            let fresh = BeatSheet::fresh(project_id);
            storage::atomic_write_json(&path, &fresh)?;
            Ok(fresh)
        }
    }

    pub fn save_beat_sheet(&self, sheet: &BeatSheet) -> Result<()> {
        let path = self.beat_sheet_path(&sheet.project_id)?;
        storage::atomic_write_json(&path, sheet)
    }

    pub fn update_beat<F>(&self, project_id: &str, beat_id: BeatId, f: F) -> Result<BeatSheet>
    where
        F: FnOnce(&mut Beat),
    {
        let mut sheet = self.load_beat_sheet(project_id)?;
        if sheet.frozen {
            return Err(QuillError::InvalidArgument(
                "beat sheet is frozen; unfreeze first".into(),
            ));
        }
        let beat = sheet
            .beats
            .iter_mut()
            .find(|b| b.id == beat_id)
            .ok_or_else(|| QuillError::NotFound(format!("beat {beat_id:?}")))?;
        f(beat);
        sheet.updated_at = Utc::now();
        self.save_beat_sheet(&sheet)?;
        Ok(sheet)
    }

    pub fn set_target_word_count(&self, project_id: &str, target: u32) -> Result<BeatSheet> {
        let mut sheet = self.load_beat_sheet(project_id)?;
        sheet.target_word_count = target.clamp(20_000, 250_000);
        sheet.updated_at = Utc::now();
        self.save_beat_sheet(&sheet)?;
        Ok(sheet)
    }

    pub fn set_frozen(&self, project_id: &str, frozen: bool) -> Result<BeatSheet> {
        let mut sheet = self.load_beat_sheet(project_id)?;
        sheet.frozen = frozen;
        sheet.updated_at = Utc::now();
        self.save_beat_sheet(&sheet)?;
        Ok(sheet)
    }

    // ---------- Scenes ----------

    pub fn load_scenes(&self, project_id: &str) -> Result<Vec<Scene>> {
        let path = self.scenes_path(project_id)?;
        let list: SceneList = if path.exists() {
            storage::atomic_read_json_or_default(&path)?
        } else {
            SceneList::default()
        };
        let mut scenes = list.scenes;
        scenes.sort_by_key(|s| s.order);
        Ok(scenes)
    }

    pub fn save_scenes(&self, project_id: &str, scenes: &[Scene]) -> Result<()> {
        let path = self.scenes_path(project_id)?;
        let list = SceneList {
            scenes: scenes.to_vec(),
        };
        storage::atomic_write_json(&path, &list)
    }

    pub fn create_scene(
        &self,
        project_id: &str,
        title: &str,
        beat_id: Option<BeatId>,
    ) -> Result<Scene> {
        let mut scenes = self.load_scenes(project_id)?;
        let order = scenes.len() as u32;
        let mut s = Scene::fresh(project_id, order, title);
        s.beat_id = beat_id;
        scenes.push(s.clone());
        self.save_scenes(project_id, &scenes)?;
        Ok(s)
    }

    pub fn delete_scene(&self, project_id: &str, scene_id: &str) -> Result<()> {
        let mut scenes = self.load_scenes(project_id)?;
        scenes.retain(|s| s.id != scene_id);
        // Re-densify order
        for (i, s) in scenes.iter_mut().enumerate() {
            s.order = i as u32;
        }
        self.save_scenes(project_id, &scenes)
    }

    pub fn reorder_scenes(&self, project_id: &str, ids_in_order: &[String]) -> Result<()> {
        let mut scenes = self.load_scenes(project_id)?;
        let mut ordered: Vec<Scene> = Vec::with_capacity(scenes.len());
        for id in ids_in_order {
            if let Some(pos) = scenes.iter().position(|s| &s.id == id) {
                let mut s = scenes.remove(pos);
                s.order = ordered.len() as u32;
                ordered.push(s);
            }
        }
        // Append any leftover scenes not present in the request (defensive)
        for mut s in scenes {
            s.order = ordered.len() as u32;
            ordered.push(s);
        }
        self.save_scenes(project_id, &ordered)
    }

    pub fn update_scene<F>(&self, project_id: &str, scene_id: &str, f: F) -> Result<Scene>
    where
        F: FnOnce(&mut Scene),
    {
        let mut scenes = self.load_scenes(project_id)?;
        let s = scenes
            .iter_mut()
            .find(|s| s.id == scene_id)
            .ok_or_else(|| QuillError::NotFound(format!("scene {scene_id}")))?;
        f(s);
        s.updated_at = Utc::now();
        let updated = s.clone();
        self.save_scenes(project_id, &scenes)?;
        Ok(updated)
    }
}

fn ensure_all_beats(sheet: &mut BeatSheet) {
    use std::collections::HashMap;
    let mut by_id: HashMap<BeatId, Beat> = sheet.beats.drain(..).map(|b| (b.id, b)).collect();
    for &id in &BeatId::ALL {
        let beat = by_id.remove(&id).unwrap_or_else(|| Beat::fresh(id));
        sheet.beats.push(beat);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::storage::ProjectStore;

    #[test]
    fn beat_sheet_initializes_with_15_beats() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let project = projects.create("Demo").unwrap();
        let store = StructureStore::new(&projects);
        let sheet = store.load_beat_sheet(&project.id).unwrap();
        assert_eq!(sheet.beats.len(), 15);
        assert!(store.beat_sheet_path(&project.id).unwrap().exists());
    }

    #[test]
    fn update_beat_persists() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let project = projects.create("Demo").unwrap();
        let store = StructureStore::new(&projects);

        let s1 = store
            .update_beat(&project.id, BeatId::Midpoint, |b| {
                b.summary = "false victory".into();
                b.satisfied = true;
            })
            .unwrap();
        let m1 = s1.beats.iter().find(|b| b.id == BeatId::Midpoint).unwrap();
        assert!(m1.satisfied);
        assert_eq!(m1.summary, "false victory");

        // Re-load fresh store, verify persistence
        let store2 = StructureStore::new(&projects);
        let s2 = store2.load_beat_sheet(&project.id).unwrap();
        let m2 = s2.beats.iter().find(|b| b.id == BeatId::Midpoint).unwrap();
        assert!(m2.satisfied);
    }

    #[test]
    fn frozen_sheet_rejects_updates() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let project = projects.create("Demo").unwrap();
        let store = StructureStore::new(&projects);
        store.set_frozen(&project.id, true).unwrap();
        let r = store.update_beat(&project.id, BeatId::Midpoint, |b| {
            b.summary = "x".into();
        });
        assert!(matches!(r, Err(QuillError::InvalidArgument(_))));
    }

    #[test]
    fn create_and_reorder_scenes() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let project = projects.create("Demo").unwrap();
        let store = StructureStore::new(&projects);
        let a = store.create_scene(&project.id, "A", None).unwrap();
        let b = store.create_scene(&project.id, "B", None).unwrap();
        let c = store.create_scene(&project.id, "C", None).unwrap();

        store
            .reorder_scenes(&project.id, &[c.id.clone(), a.id.clone(), b.id.clone()])
            .unwrap();
        let scenes = store.load_scenes(&project.id).unwrap();
        assert_eq!(scenes[0].title, "C");
        assert_eq!(scenes[1].title, "A");
        assert_eq!(scenes[2].title, "B");
    }

    #[test]
    fn delete_scene_redensifies_order() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let project = projects.create("Demo").unwrap();
        let store = StructureStore::new(&projects);
        let a = store.create_scene(&project.id, "A", None).unwrap();
        let _ = store.create_scene(&project.id, "B", None).unwrap();
        let _ = store.create_scene(&project.id, "C", None).unwrap();
        store.delete_scene(&project.id, &a.id).unwrap();
        let scenes = store.load_scenes(&project.id).unwrap();
        assert_eq!(scenes.len(), 2);
        assert_eq!(scenes[0].order, 0);
        assert_eq!(scenes[1].order, 1);
    }
}
