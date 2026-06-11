//! Per-project JSON storage for the Character Bible and the Idea Park.
//!
//! Files (created on first read/write):
//!   <project>/bible/characters.json
//!   <project>/bible/ideas.json

use crate::error::{QuillError, Result};
use crate::models::brain::{
    Character, CharacterPatch, Idea, IdeaPatch, Thread, ThreadPatch, ThreadStatus, WorldEntry,
    WorldEntryPatch, WorldKind,
};
use crate::services::storage::{self, ProjectStore};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct CharacterFile {
    pub characters: Vec<Character>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct IdeaFile {
    pub ideas: Vec<Idea>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct ThreadFile {
    pub threads: Vec<Thread>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct WorldFile {
    pub entries: Vec<WorldEntry>,
}

pub struct CharacterStore<'a> {
    projects: &'a ProjectStore,
}

impl<'a> CharacterStore<'a> {
    pub fn new(projects: &'a ProjectStore) -> Self {
        Self { projects }
    }

    fn path(&self, project_id: &str) -> Result<PathBuf> {
        let dir = self.projects.root_dir(project_id)?.join("bible");
        std::fs::create_dir_all(&dir)?;
        Ok(dir.join("characters.json"))
    }

    pub fn list(&self, project_id: &str) -> Result<Vec<Character>> {
        let p = self.path(project_id)?;
        let f: CharacterFile = storage::atomic_read_json_or_default(&p)?;
        Ok(f.characters)
    }

    pub fn save(&self, project_id: &str, characters: &[Character]) -> Result<()> {
        let p = self.path(project_id)?;
        storage::atomic_write_json(
            &p,
            &CharacterFile {
                characters: characters.to_vec(),
            },
        )
    }

    pub fn create(&self, project_id: &str, name: &str) -> Result<Character> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(QuillError::InvalidArgument(
                "character name cannot be empty".into(),
            ));
        }
        let mut chars = self.list(project_id)?;
        let c = Character::fresh(project_id, trimmed);
        chars.push(c.clone());
        self.save(project_id, &chars)?;
        Ok(c)
    }

    pub fn delete(&self, project_id: &str, id: &str) -> Result<()> {
        let mut chars = self.list(project_id)?;
        let before = chars.len();
        chars.retain(|c| c.id != id);
        if chars.len() == before {
            return Err(QuillError::NotFound(format!("character {id}")));
        }
        self.save(project_id, &chars)
    }

    pub fn update(&self, project_id: &str, id: &str, patch: CharacterPatch) -> Result<Character> {
        let mut chars = self.list(project_id)?;
        let c = chars
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or_else(|| QuillError::NotFound(format!("character {id}")))?;
        patch.apply(c);
        c.updated_at = Utc::now();
        let updated = c.clone();
        self.save(project_id, &chars)?;
        Ok(updated)
    }
}

pub struct IdeaStore<'a> {
    projects: &'a ProjectStore,
}

impl<'a> IdeaStore<'a> {
    pub fn new(projects: &'a ProjectStore) -> Self {
        Self { projects }
    }

    fn path(&self, project_id: &str) -> Result<PathBuf> {
        let dir = self.projects.root_dir(project_id)?.join("bible");
        std::fs::create_dir_all(&dir)?;
        Ok(dir.join("ideas.json"))
    }

    pub fn list(&self, project_id: &str) -> Result<Vec<Idea>> {
        let p = self.path(project_id)?;
        let f: IdeaFile = storage::atomic_read_json_or_default(&p)?;
        Ok(f.ideas)
    }

    pub fn save(&self, project_id: &str, ideas: &[Idea]) -> Result<()> {
        let p = self.path(project_id)?;
        storage::atomic_write_json(
            &p,
            &IdeaFile {
                ideas: ideas.to_vec(),
            },
        )
    }

    pub fn create(&self, project_id: &str, text: &str) -> Result<Idea> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(QuillError::InvalidArgument(
                "idea text cannot be empty".into(),
            ));
        }
        let mut ideas = self.list(project_id)?;
        let i = Idea::fresh(project_id, trimmed);
        ideas.push(i.clone());
        self.save(project_id, &ideas)?;
        Ok(i)
    }

    pub fn delete(&self, project_id: &str, id: &str) -> Result<()> {
        let mut ideas = self.list(project_id)?;
        let before = ideas.len();
        ideas.retain(|i| i.id != id);
        if ideas.len() == before {
            return Err(QuillError::NotFound(format!("idea {id}")));
        }
        self.save(project_id, &ideas)
    }

    pub fn update(&self, project_id: &str, id: &str, patch: IdeaPatch) -> Result<Idea> {
        let mut ideas = self.list(project_id)?;
        let i = ideas
            .iter_mut()
            .find(|i| i.id == id)
            .ok_or_else(|| QuillError::NotFound(format!("idea {id}")))?;
        patch.apply(i);
        i.updated_at = Utc::now();
        let updated = i.clone();
        self.save(project_id, &ideas)?;
        Ok(updated)
    }

    /// Pick up to `limit` ideas whose tags target the current draft.
    ///
    /// Tag conventions (matched case-insensitively):
    ///   - `beat:<beat_id>` → matches if `beat_id` equals the active beat
    ///   - `scene:<scene_id>` → matches if `scene_id` equals the active scene
    ///   - `pov:<name>` → matches if `name` is contained in (or contains)
    ///     the POV name, both lowercased
    ///
    /// `do_not_send=true` ideas are excluded — they should never leave the
    /// machine. Newest ideas come first; if multiple ideas tie, the more
    /// recently created one wins. Returns at most `limit` items.
    pub fn relevant_for_draft(
        &self,
        project_id: &str,
        beat_id: Option<&str>,
        scene_id: Option<&str>,
        pov: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Idea>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let ideas = self.list(project_id)?;
        let pov_lower = pov.map(|s| s.to_lowercase());
        let mut matched: Vec<Idea> = ideas
            .into_iter()
            .filter(|i| !i.do_not_send)
            .filter(|i| {
                i.tags.iter().any(|raw| {
                    let tag = raw.trim().to_lowercase();
                    if let Some(beat) = beat_id {
                        if tag == format!("beat:{}", beat.to_lowercase()) {
                            return true;
                        }
                    }
                    if let Some(scene) = scene_id {
                        if tag == format!("scene:{}", scene.to_lowercase()) {
                            return true;
                        }
                    }
                    if let Some(pov) = pov_lower.as_deref() {
                        if let Some(rest) = tag.strip_prefix("pov:") {
                            let rest = rest.trim();
                            if !rest.is_empty() && (pov.contains(rest) || rest.contains(pov)) {
                                return true;
                            }
                        }
                    }
                    false
                })
            })
            .collect();
        // Most recent first.
        matched.sort_by_key(|i| std::cmp::Reverse(i.created_at));
        matched.truncate(limit);
        Ok(matched)
    }
}

pub struct ThreadStore<'a> {
    projects: &'a ProjectStore,
}

impl<'a> ThreadStore<'a> {
    pub fn new(projects: &'a ProjectStore) -> Self {
        Self { projects }
    }

    fn path(&self, project_id: &str) -> Result<PathBuf> {
        let dir = self.projects.root_dir(project_id)?.join("bible");
        std::fs::create_dir_all(&dir)?;
        Ok(dir.join("threads.json"))
    }

    pub fn list(&self, project_id: &str) -> Result<Vec<Thread>> {
        let p = self.path(project_id)?;
        let f: ThreadFile = storage::atomic_read_json_or_default(&p)?;
        Ok(f.threads)
    }

    pub fn save(&self, project_id: &str, threads: &[Thread]) -> Result<()> {
        let p = self.path(project_id)?;
        storage::atomic_write_json(
            &p,
            &ThreadFile {
                threads: threads.to_vec(),
            },
        )
    }

    pub fn create(&self, project_id: &str, title: &str) -> Result<Thread> {
        let trimmed = title.trim();
        if trimmed.is_empty() {
            return Err(QuillError::InvalidArgument(
                "thread title cannot be empty".into(),
            ));
        }
        let mut threads = self.list(project_id)?;
        let t = Thread::fresh(project_id, trimmed);
        threads.push(t.clone());
        self.save(project_id, &threads)?;
        Ok(t)
    }

    pub fn delete(&self, project_id: &str, id: &str) -> Result<()> {
        let mut threads = self.list(project_id)?;
        let before = threads.len();
        threads.retain(|t| t.id != id);
        if threads.len() == before {
            return Err(QuillError::NotFound(format!("thread {id}")));
        }
        self.save(project_id, &threads)
    }

    pub fn update(&self, project_id: &str, id: &str, patch: ThreadPatch) -> Result<Thread> {
        let mut threads = self.list(project_id)?;
        let t = threads
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(|| QuillError::NotFound(format!("thread {id}")))?;
        patch.apply(t);
        t.updated_at = Utc::now();
        let updated = t.clone();
        self.save(project_id, &threads)?;
        Ok(updated)
    }

    /// Threads with status Open or Advancing — what the drafting
    /// orchestrator should consider "in motion". Returns most recently
    /// updated first.
    pub fn in_motion(&self, project_id: &str) -> Result<Vec<Thread>> {
        let mut threads = self.list(project_id)?;
        threads.retain(|t| matches!(t.status, ThreadStatus::Open | ThreadStatus::Advancing));
        threads.sort_by_key(|t| std::cmp::Reverse(t.updated_at));
        Ok(threads)
    }
}

/// World Bible store — places, factions, and lore concepts.
/// File: `<project>/bible/world.json`.
pub struct WorldStore<'a> {
    projects: &'a ProjectStore,
}

impl<'a> WorldStore<'a> {
    pub fn new(projects: &'a ProjectStore) -> Self {
        Self { projects }
    }

    fn path(&self, project_id: &str) -> Result<PathBuf> {
        let dir = self.projects.root_dir(project_id)?.join("bible");
        std::fs::create_dir_all(&dir)?;
        Ok(dir.join("world.json"))
    }

    pub fn list(&self, project_id: &str) -> Result<Vec<WorldEntry>> {
        let p = self.path(project_id)?;
        let f: WorldFile = storage::atomic_read_json_or_default(&p)?;
        Ok(f.entries)
    }

    pub fn save(&self, project_id: &str, entries: &[WorldEntry]) -> Result<()> {
        let p = self.path(project_id)?;
        storage::atomic_write_json(
            &p,
            &WorldFile {
                entries: entries.to_vec(),
            },
        )
    }

    pub fn create(&self, project_id: &str, name: &str, kind: WorldKind) -> Result<WorldEntry> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(QuillError::InvalidArgument(
                "world entry name cannot be empty".into(),
            ));
        }
        let mut entries = self.list(project_id)?;
        let w = WorldEntry::fresh(project_id, trimmed, kind);
        entries.push(w.clone());
        self.save(project_id, &entries)?;
        Ok(w)
    }

    pub fn delete(&self, project_id: &str, id: &str) -> Result<()> {
        let mut entries = self.list(project_id)?;
        let before = entries.len();
        entries.retain(|w| w.id != id);
        if entries.len() == before {
            return Err(QuillError::NotFound(format!("world entry {id}")));
        }
        self.save(project_id, &entries)
    }

    pub fn update(
        &self,
        project_id: &str,
        id: &str,
        patch: WorldEntryPatch,
    ) -> Result<WorldEntry> {
        let mut entries = self.list(project_id)?;
        let w = entries
            .iter_mut()
            .find(|w| w.id == id)
            .ok_or_else(|| QuillError::NotFound(format!("world entry {id}")))?;
        patch.apply(w);
        w.updated_at = Utc::now();
        let updated = w.clone();
        self.save(project_id, &entries)?;
        Ok(updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::storage::ProjectStore;

    #[test]
    fn character_create_list_update_delete() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let p = projects.create("Demo").unwrap();
        let store = CharacterStore::new(&projects);

        assert!(store.list(&p.id).unwrap().is_empty());

        let c = store.create(&p.id, "Kaelan").unwrap();
        assert_eq!(store.list(&p.id).unwrap().len(), 1);

        let patch = CharacterPatch {
            motivation: Some("revenge".into()),
            aliases: Some(vec!["Kael".into()]),
            ..CharacterPatch::default()
        };
        let updated = store.update(&p.id, &c.id, patch).unwrap();
        assert_eq!(updated.motivation, "revenge");
        assert_eq!(updated.aliases, vec!["Kael".to_string()]);

        store.delete(&p.id, &c.id).unwrap();
        assert!(store.list(&p.id).unwrap().is_empty());
    }

    #[test]
    fn idea_create_list_update_delete() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let p = projects.create("Demo").unwrap();
        let store = IdeaStore::new(&projects);

        let i = store
            .create(&p.id, "What if Tarn freezes at midnight?")
            .unwrap();
        let updated = store
            .update(
                &p.id,
                &i.id,
                IdeaPatch {
                    tags: Some(vec!["worldbuilding".into(), "magic".into()]),
                    ..IdeaPatch::default()
                },
            )
            .unwrap();
        assert_eq!(updated.tags.len(), 2);

        store.delete(&p.id, &i.id).unwrap();
        assert!(store.list(&p.id).unwrap().is_empty());
    }

    #[test]
    fn rejects_empty_strings() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let p = projects.create("Demo").unwrap();
        let cs = CharacterStore::new(&projects);
        let is = IdeaStore::new(&projects);
        assert!(matches!(
            cs.create(&p.id, "  "),
            Err(QuillError::InvalidArgument(_))
        ));
        assert!(matches!(
            is.create(&p.id, ""),
            Err(QuillError::InvalidArgument(_))
        ));
    }

    fn add_idea(store: &IdeaStore<'_>, pid: &str, text: &str, tags: &[&str], dns: bool) {
        let i = store.create(pid, text).unwrap();
        store
            .update(
                pid,
                &i.id,
                IdeaPatch {
                    tags: Some(tags.iter().map(|s| s.to_string()).collect()),
                    do_not_send: Some(dns),
                    ..IdeaPatch::default()
                },
            )
            .unwrap();
    }

    #[test]
    fn relevant_for_draft_matches_beat_tag() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let p = projects.create("Demo").unwrap();
        let store = IdeaStore::new(&projects);
        add_idea(
            &store,
            &p.id,
            "kaelan flinches at fire",
            &["beat:catalyst"],
            false,
        );
        add_idea(&store, &p.id, "unrelated thought", &["random"], false);
        let hits = store
            .relevant_for_draft(&p.id, Some("catalyst"), None, None, 10)
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].text.contains("flinches"));
    }

    #[test]
    fn relevant_for_draft_matches_pov_loosely() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let p = projects.create("Demo").unwrap();
        let store = IdeaStore::new(&projects);
        // tag is "pov:kaelan", scene POV string is "Kaelan, 3rd-limited"
        add_idea(&store, &p.id, "voice idea for Kael", &["pov:kaelan"], false);
        let hits = store
            .relevant_for_draft(&p.id, None, None, Some("Kaelan, 3rd-limited"), 10)
            .unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn relevant_for_draft_excludes_do_not_send() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let p = projects.create("Demo").unwrap();
        let store = IdeaStore::new(&projects);
        add_idea(&store, &p.id, "secret reveal", &["beat:finale"], true);
        let hits = store
            .relevant_for_draft(&p.id, Some("finale"), None, None, 10)
            .unwrap();
        assert!(hits.is_empty(), "do_not_send idea must not leak");
    }

    #[test]
    fn thread_create_update_in_motion_filters_resolved() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let p = projects.create("Demo").unwrap();
        let store = ThreadStore::new(&projects);

        let t1 = store.create(&p.id, "Kaelan's vow").unwrap();
        let t2 = store.create(&p.id, "Forgotten god in the well").unwrap();
        // Mark t1 as resolved.
        store
            .update(
                &p.id,
                &t1.id,
                ThreadPatch {
                    status: Some(ThreadStatus::Resolved),
                    ..ThreadPatch::default()
                },
            )
            .unwrap();
        let motion = store.in_motion(&p.id).unwrap();
        assert_eq!(motion.len(), 1);
        assert_eq!(motion[0].id, t2.id);
    }

    #[test]
    fn thread_rejects_empty_title() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let p = projects.create("Demo").unwrap();
        let store = ThreadStore::new(&projects);
        assert!(matches!(
            store.create(&p.id, "  "),
            Err(QuillError::InvalidArgument(_))
        ));
    }

    #[test]
    fn relevant_for_draft_honors_limit_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let p = projects.create("Demo").unwrap();
        let store = IdeaStore::new(&projects);
        for n in 0..7 {
            add_idea(
                &store,
                &p.id,
                &format!("idea {n}"),
                &["beat:midpoint"],
                false,
            );
            // Sleep a tiny bit so created_at order is deterministic.
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let hits = store
            .relevant_for_draft(&p.id, Some("midpoint"), None, None, 3)
            .unwrap();
        assert_eq!(hits.len(), 3);
        // Newest (idea 6) first.
        assert!(hits[0].text.ends_with("6"));
        assert!(hits[1].text.ends_with("5"));
        assert!(hits[2].text.ends_with("4"));
    }
}
