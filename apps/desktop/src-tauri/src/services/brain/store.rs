//! Per-project JSON storage for the Character Bible and the Idea Park.
//!
//! Files (created on first read/write):
//!   <project>/bible/characters.json
//!   <project>/bible/ideas.json

use crate::error::{QuillError, Result};
use crate::models::brain::{Character, CharacterPatch, Idea, IdeaPatch};
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
}
