//! Per-project beat sheet + chapter + scene list storage.
//!
//! Files:
//!   <project>/structure/beat_sheet.json
//!   <project>/structure/chapters.json
//!   <project>/structure/scenes.json
//!
//! Chapter invariants the store maintains:
//! - If any scene exists, at least one chapter exists (the migration
//!   creates "Chapter 1" and adopts orphans).
//! - Every scene's `chapter_id` points at a real chapter.
//! - `Scene.order` stays the GLOBAL manuscript order, with each chapter's
//!   scenes contiguous and chapter blocks in chapter order — so compile,
//!   search, progress, and the per-scene file naming all keep working on
//!   the flat ordered list with no knowledge of chapters.

use crate::error::{QuillError, Result};
use crate::models::structure::{
    Beat, BeatId, BeatSheet, Chapter, ChapterList, ChapterPatch, Scene, SceneList,
};
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

    fn chapters_path(&self, project_id: &str) -> Result<PathBuf> {
        Ok(self.dir(project_id)?.join("chapters.json"))
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
        self.create_scene_in_chapter(project_id, title, beat_id, None)
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

    /// Legacy flat reorder. Kept for API compatibility; the rail now uses
    /// `move_scene`. Applies the requested order, then re-normalizes so
    /// chapter blocks stay contiguous (a flat order interleaving chapters
    /// is regrouped, preserving relative order within each chapter).
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
        let mut chapters = self.load_chapters(project_id)?;
        if !chapters.is_empty() {
            normalize(&mut chapters, &mut ordered);
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

    // ---------- Chapters ----------

    pub fn load_chapters(&self, project_id: &str) -> Result<Vec<Chapter>> {
        let path = self.chapters_path(project_id)?;
        let list: ChapterList = if path.exists() {
            storage::atomic_read_json_or_default(&path)?
        } else {
            ChapterList::default()
        };
        let mut chapters = list.chapters;
        chapters.sort_by_key(|c| c.order);
        Ok(chapters)
    }

    pub fn save_chapters(&self, project_id: &str, chapters: &[Chapter]) -> Result<()> {
        let path = self.chapters_path(project_id)?;
        storage::atomic_write_json(
            &path,
            &ChapterList {
                chapters: chapters.to_vec(),
            },
        )
    }

    /// Migration + self-heal, idempotent and cheap when nothing's wrong:
    /// - scenes exist but no chapters → create "Chapter 1" and adopt all
    /// - scenes with a missing/dangling `chapter_id` → adopt into the
    ///   first chapter
    ///
    /// Returns the (possibly freshly created) chapter list.
    pub fn ensure_chapters(&self, project_id: &str) -> Result<Vec<Chapter>> {
        let mut chapters = self.load_chapters(project_id)?;
        let mut scenes = self.load_scenes(project_id)?;

        if chapters.is_empty() {
            if scenes.is_empty() {
                return Ok(chapters); // empty project — nothing to do
            }
            chapters.push(Chapter::fresh(project_id, 0, "Chapter 1"));
            self.save_chapters(project_id, &chapters)?;
        }

        let valid: std::collections::HashSet<&str> =
            chapters.iter().map(|c| c.id.as_str()).collect();
        let first_id = chapters[0].id.clone();
        let mut dirty = false;
        for s in &mut scenes {
            let ok = s
                .chapter_id
                .as_deref()
                .map(|c| valid.contains(c))
                .unwrap_or(false);
            if !ok {
                s.chapter_id = Some(first_id.clone());
                dirty = true;
            }
        }
        if dirty {
            normalize(&mut chapters, &mut scenes);
            self.save_scenes(project_id, &scenes)?;
            self.save_chapters(project_id, &chapters)?;
        }
        Ok(chapters)
    }

    pub fn create_chapter(&self, project_id: &str, title: &str) -> Result<Chapter> {
        let trimmed = title.trim();
        if trimmed.is_empty() {
            return Err(QuillError::InvalidArgument(
                "chapter title cannot be empty".into(),
            ));
        }
        let mut chapters = self.ensure_chapters(project_id)?;
        let c = Chapter::fresh(project_id, chapters.len() as u32, trimmed);
        chapters.push(c.clone());
        self.save_chapters(project_id, &chapters)?;
        Ok(c)
    }

    pub fn update_chapter(
        &self,
        project_id: &str,
        chapter_id: &str,
        patch: ChapterPatch,
    ) -> Result<Chapter> {
        let mut chapters = self.load_chapters(project_id)?;
        let c = chapters
            .iter_mut()
            .find(|c| c.id == chapter_id)
            .ok_or_else(|| QuillError::NotFound(format!("chapter {chapter_id}")))?;
        patch.apply(c);
        c.updated_at = Utc::now();
        let updated = c.clone();
        self.save_chapters(project_id, &chapters)?;
        Ok(updated)
    }

    /// Delete a chapter. Its scenes move to the neighboring chapter
    /// (previous if any, otherwise next). Deleting the only chapter is
    /// allowed only when it has no scenes.
    pub fn delete_chapter(&self, project_id: &str, chapter_id: &str) -> Result<()> {
        let mut chapters = self.load_chapters(project_id)?;
        let mut scenes = self.load_scenes(project_id)?;
        let pos = chapters
            .iter()
            .position(|c| c.id == chapter_id)
            .ok_or_else(|| QuillError::NotFound(format!("chapter {chapter_id}")))?;

        let has_scenes = scenes
            .iter()
            .any(|s| s.chapter_id.as_deref() == Some(chapter_id));
        if has_scenes {
            if chapters.len() == 1 {
                return Err(QuillError::InvalidArgument(
                    "cannot delete the only chapter while it still has scenes".into(),
                ));
            }
            let target = if pos > 0 {
                chapters[pos - 1].id.clone()
            } else {
                chapters[pos + 1].id.clone()
            };
            for s in &mut scenes {
                if s.chapter_id.as_deref() == Some(chapter_id) {
                    s.chapter_id = Some(target.clone());
                }
            }
        }
        chapters.remove(pos);
        normalize(&mut chapters, &mut scenes);
        self.save_chapters(project_id, &chapters)?;
        self.save_scenes(project_id, &scenes)
    }

    /// Reorder chapters; their scene blocks follow automatically (global
    /// scene order is rebuilt from the new chapter order).
    pub fn reorder_chapters(&self, project_id: &str, ids_in_order: &[String]) -> Result<()> {
        let mut chapters = self.load_chapters(project_id)?;
        let mut scenes = self.load_scenes(project_id)?;
        let mut ordered: Vec<Chapter> = Vec::with_capacity(chapters.len());
        for id in ids_in_order {
            if let Some(pos) = chapters.iter().position(|c| &c.id == id) {
                ordered.push(chapters.remove(pos));
            }
        }
        ordered.append(&mut chapters); // defensive: keep any unmentioned
        normalize(&mut ordered, &mut scenes);
        self.save_chapters(project_id, &ordered)?;
        self.save_scenes(project_id, &scenes)
    }

    /// Move a scene into `chapter_id` at `index` (0-based position within
    /// that chapter; clamped). Works for same-chapter reorders too.
    pub fn move_scene(
        &self,
        project_id: &str,
        scene_id: &str,
        chapter_id: &str,
        index: u32,
    ) -> Result<()> {
        let chapters = self.ensure_chapters(project_id)?;
        if !chapters.iter().any(|c| c.id == chapter_id) {
            return Err(QuillError::NotFound(format!("chapter {chapter_id}")));
        }
        let scenes = self.load_scenes(project_id)?;

        // Bucket scenes by chapter (chapter order), pull the moving scene
        // out, then reinsert at the requested slot and flatten.
        let mut moving: Option<Scene> = None;
        let mut buckets: Vec<(String, Vec<Scene>)> = chapters
            .iter()
            .map(|c| (c.id.clone(), Vec::new()))
            .collect();
        for s in scenes {
            if s.id == scene_id {
                moving = Some(s);
                continue;
            }
            let cid = s.chapter_id.clone().unwrap_or_default();
            if let Some((_, bucket)) = buckets.iter_mut().find(|(id, _)| *id == cid) {
                bucket.push(s);
            }
        }
        let mut moving = moving.ok_or_else(|| QuillError::NotFound(format!("scene {scene_id}")))?;
        moving.chapter_id = Some(chapter_id.to_string());
        moving.updated_at = Utc::now();
        let (_, target) = buckets
            .iter_mut()
            .find(|(id, _)| id == chapter_id)
            .expect("validated above");
        let at = (index as usize).min(target.len());
        target.insert(at, moving);

        let mut flat: Vec<Scene> = buckets.into_iter().flat_map(|(_, b)| b).collect();
        for (i, s) in flat.iter_mut().enumerate() {
            s.order = i as u32;
        }
        self.save_scenes(project_id, &flat)
    }

    /// Create a scene inside `chapter_id` (or the last chapter when None;
    /// an empty project gets "Chapter 1" automatically). The scene lands
    /// at the end of the chapter's block.
    pub fn create_scene_in_chapter(
        &self,
        project_id: &str,
        title: &str,
        beat_id: Option<BeatId>,
        chapter_id: Option<&str>,
    ) -> Result<Scene> {
        let mut chapters = self.ensure_chapters(project_id)?;
        if chapters.is_empty() {
            // Empty project: bootstrap the first chapter.
            let c = Chapter::fresh(project_id, 0, "Chapter 1");
            chapters.push(c);
            self.save_chapters(project_id, &chapters)?;
        }
        let target = match chapter_id {
            Some(id) => chapters
                .iter()
                .find(|c| c.id == id)
                .ok_or_else(|| QuillError::NotFound(format!("chapter {id}")))?
                .id
                .clone(),
            None => chapters.last().expect("non-empty").id.clone(),
        };

        let mut scenes = self.load_scenes(project_id)?;
        let mut s = Scene::fresh(project_id, 0, title);
        s.beat_id = beat_id;
        s.chapter_id = Some(target);
        scenes.push(s.clone());
        normalize(&mut chapters, &mut scenes);
        self.save_scenes(project_id, &scenes)?;
        // Return the normalized copy so `order` is correct.
        let created = scenes
            .iter()
            .find(|x| x.id == s.id)
            .cloned()
            .expect("just inserted");
        Ok(created)
    }
}

/// Rebuild global scene order from chapter order: chapters get dense
/// 0-based orders; scenes are stably grouped by chapter (preserving their
/// relative order within each chapter) and re-densified globally.
fn normalize(chapters: &mut [Chapter], scenes: &mut [Scene]) {
    for (i, c) in chapters.iter_mut().enumerate() {
        c.order = i as u32;
    }
    let chapter_pos = |cid: &Option<String>| -> usize {
        cid.as_deref()
            .and_then(|id| chapters.iter().position(|c| c.id == id))
            .unwrap_or(usize::MAX) // orphans sink to the end; ensure_chapters heals them
    };
    scenes.sort_by_key(|s| (chapter_pos(&s.chapter_id), s.order));
    for (i, s) in scenes.iter_mut().enumerate() {
        s.order = i as u32;
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

    // ---------- Chapter tests ----------

    fn chapter_of<'a>(scenes: &'a [Scene], title: &str) -> &'a str {
        scenes
            .iter()
            .find(|s| s.title == title)
            .unwrap()
            .chapter_id
            .as_deref()
            .unwrap()
    }

    #[test]
    fn migration_adopts_legacy_scenes_into_chapter_one() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let project = projects.create("Demo").unwrap();
        let store = StructureStore::new(&projects);

        // Simulate legacy data: scenes saved with no chapter_id and no
        // chapters file (pre-chapters builds).
        let mut a = Scene::fresh(&project.id, 0, "A");
        let mut b = Scene::fresh(&project.id, 1, "B");
        a.chapter_id = None;
        b.chapter_id = None;
        store.save_scenes(&project.id, &[a, b]).unwrap();

        let chapters = store.ensure_chapters(&project.id).unwrap();
        assert_eq!(chapters.len(), 1);
        assert_eq!(chapters[0].title, "Chapter 1");

        let scenes = store.load_scenes(&project.id).unwrap();
        assert!(scenes
            .iter()
            .all(|s| s.chapter_id.as_deref() == Some(chapters[0].id.as_str())));
        // Idempotent.
        let again = store.ensure_chapters(&project.id).unwrap();
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].id, chapters[0].id);
    }

    #[test]
    fn create_scene_lands_in_requested_chapter_block() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let project = projects.create("Demo").unwrap();
        let store = StructureStore::new(&projects);

        // First scene bootstraps Chapter 1.
        let a = store.create_scene(&project.id, "A", None).unwrap();
        assert!(a.chapter_id.is_some());
        let ch2 = store.create_chapter(&project.id, "Chapter 2").unwrap();
        // New scene into chapter 2, then another into chapter 1 — global
        // order must keep chapter blocks contiguous.
        store
            .create_scene_in_chapter(&project.id, "C", None, Some(&ch2.id))
            .unwrap();
        store
            .create_scene_in_chapter(&project.id, "B", None, a.chapter_id.as_deref())
            .unwrap();

        let scenes = store.load_scenes(&project.id).unwrap();
        let titles: Vec<&str> = scenes.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(
            titles,
            vec!["A", "B", "C"],
            "chapter 1 block precedes chapter 2"
        );
        assert_eq!(scenes[0].order, 0);
        assert_eq!(scenes[2].order, 2);
    }

    #[test]
    fn move_scene_across_chapters_and_within() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let project = projects.create("Demo").unwrap();
        let store = StructureStore::new(&projects);

        let a = store.create_scene(&project.id, "A", None).unwrap();
        let _b = store.create_scene(&project.id, "B", None).unwrap();
        let ch1 = a.chapter_id.clone().unwrap();
        let ch2 = store.create_chapter(&project.id, "Chapter 2").unwrap();

        // Move A to chapter 2 (index 0): order becomes B, A.
        store.move_scene(&project.id, &a.id, &ch2.id, 0).unwrap();
        let scenes = store.load_scenes(&project.id).unwrap();
        let titles: Vec<&str> = scenes.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, vec!["B", "A"]);
        assert_eq!(chapter_of(&scenes, "A"), ch2.id);

        // Move A back to chapter 1 at index 0: order A, B.
        store.move_scene(&project.id, &a.id, &ch1, 0).unwrap();
        let scenes = store.load_scenes(&project.id).unwrap();
        let titles: Vec<&str> = scenes.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, vec!["A", "B"]);

        // Within-chapter move: A to index 1 (past B) → B, A.
        store.move_scene(&project.id, &a.id, &ch1, 1).unwrap();
        let scenes = store.load_scenes(&project.id).unwrap();
        let titles: Vec<&str> = scenes.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, vec!["B", "A"]);
    }

    #[test]
    fn reorder_chapters_moves_scene_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let project = projects.create("Demo").unwrap();
        let store = StructureStore::new(&projects);

        let a = store.create_scene(&project.id, "A", None).unwrap();
        let ch1 = a.chapter_id.clone().unwrap();
        let ch2 = store.create_chapter(&project.id, "Chapter 2").unwrap();
        store
            .create_scene_in_chapter(&project.id, "B", None, Some(&ch2.id))
            .unwrap();

        store
            .reorder_chapters(&project.id, &[ch2.id.clone(), ch1.clone()])
            .unwrap();
        let chapters = store.load_chapters(&project.id).unwrap();
        assert_eq!(chapters[0].id, ch2.id);
        assert_eq!(chapters[0].order, 0);
        let scenes = store.load_scenes(&project.id).unwrap();
        let titles: Vec<&str> = scenes.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, vec!["B", "A"], "scene blocks follow their chapters");
    }

    #[test]
    fn delete_chapter_merges_scenes_into_neighbor() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let project = projects.create("Demo").unwrap();
        let store = StructureStore::new(&projects);

        let a = store.create_scene(&project.id, "A", None).unwrap();
        let ch1 = a.chapter_id.clone().unwrap();
        let ch2 = store.create_chapter(&project.id, "Chapter 2").unwrap();
        store
            .create_scene_in_chapter(&project.id, "B", None, Some(&ch2.id))
            .unwrap();

        store.delete_chapter(&project.id, &ch2.id).unwrap();
        let scenes = store.load_scenes(&project.id).unwrap();
        assert!(scenes
            .iter()
            .all(|s| s.chapter_id.as_deref() == Some(ch1.as_str())));
        assert_eq!(store.load_chapters(&project.id).unwrap().len(), 1);

        // Deleting the only chapter while it has scenes is refused.
        let r = store.delete_chapter(&project.id, &ch1);
        assert!(matches!(r, Err(QuillError::InvalidArgument(_))));
    }

    #[test]
    fn chapter_patch_clears_target_with_double_option() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let project = projects.create("Demo").unwrap();
        let store = StructureStore::new(&projects);
        let ch = store.create_chapter(&project.id, "Chapter 1").unwrap();

        let updated = store
            .update_chapter(
                &project.id,
                &ch.id,
                ChapterPatch {
                    target_word_count: Some(Some(2_500)),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(updated.target_word_count, Some(2_500));

        let cleared = store
            .update_chapter(
                &project.id,
                &ch.id,
                ChapterPatch {
                    target_word_count: Some(None),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(cleared.target_word_count, None);
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
