//! Per-project filesystem layout and CRUD.

use crate::error::{QuillError, Result};
use crate::models::{Project, ProjectPatch};
use std::path::{Path, PathBuf};

const PROJECTS_DIR: &str = "projects";
const PROJECT_FILE: &str = "project.json";

pub struct ProjectStore {
    root: PathBuf,
}

impl ProjectStore {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            root: data_dir.join(PROJECTS_DIR),
        }
    }

    fn project_dir(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    fn project_file(&self, id: &str) -> PathBuf {
        self.project_dir(id).join(PROJECT_FILE)
    }

    /// Create the directory layout for a new project.
    ///
    /// Layout (Phase 0): only the metadata file + manuscript folder. Later
    /// phases add `canon/`, `structure/`, `bible/`, `voice/`, `vectors/`.
    pub fn create(&self, name: &str) -> Result<Project> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(QuillError::InvalidArgument(
                "project name cannot be empty".into(),
            ));
        }
        if trimmed.chars().count() > 200 {
            return Err(QuillError::InvalidArgument(
                "project name too long (max 200 characters)".into(),
            ));
        }

        let project = Project::new(trimmed.to_string());

        let dir = self.project_dir(&project.id);
        std::fs::create_dir_all(dir.join("manuscript"))?;
        std::fs::create_dir_all(dir.join("canon"))?;
        std::fs::create_dir_all(dir.join("structure"))?;
        std::fs::create_dir_all(dir.join("bible"))?;
        std::fs::create_dir_all(dir.join("voice"))?;

        super::atomic::write_json(&self.project_file(&project.id), &project)?;

        // Seed an empty front-matter file so the manuscript folder isn't a
        // ghost. Plain Markdown — the user can open it in any editor.
        let fm = dir.join("manuscript").join("00-front-matter.md");
        if !fm.exists() {
            super::atomic::write_bytes(
                &fm,
                format!(
                    "# {}\n\n_Working draft — Quill will populate this as you write._\n",
                    project.name
                )
                .as_bytes(),
            )?;
        }

        Ok(project)
    }

    pub fn list(&self) -> Result<Vec<Project>> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&self.root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let pf = entry.path().join(PROJECT_FILE);
            if !pf.exists() {
                continue;
            }
            match super::atomic::read_json::<Project>(&pf) {
                Ok(p) => out.push(p),
                Err(e) => {
                    tracing::warn!(?pf, error = %crate::error::DisplayErr(&e),
                                   "skipping unreadable project file");
                }
            }
        }
        out.sort_by_key(|p| std::cmp::Reverse(p.updated_at));
        Ok(out)
    }

    pub fn open(&self, id: &str) -> Result<Project> {
        let pf = self.project_file(id);
        if !pf.exists() {
            return Err(QuillError::NotFound(format!("project {id}")));
        }
        super::atomic::read_json(&pf)
    }

    pub fn root_dir(&self, id: &str) -> Result<PathBuf> {
        let dir = self.project_dir(id);
        if !dir.exists() {
            return Err(QuillError::NotFound(format!("project {id}")));
        }
        Ok(dir)
    }

    /// Apply a partial update to a project's metadata and persist it.
    pub fn update(&self, id: &str, patch: ProjectPatch) -> Result<Project> {
        let mut project = self.open(id)?;
        patch.apply(&mut project);
        super::atomic::write_json(&self.project_file(id), &project)?;
        Ok(project)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_list() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let p1 = store.create("Storm of Ravens").unwrap();
        let p2 = store.create("The Hollow King").unwrap();

        let list = store.list().unwrap();
        assert_eq!(list.len(), 2);
        // Most recent first
        assert_eq!(list[0].id, p2.id);
        assert_eq!(list[1].id, p1.id);

        // Manuscript dir + seed file exist
        let ms = dir
            .path()
            .join(PROJECTS_DIR)
            .join(&p1.id)
            .join("manuscript")
            .join("00-front-matter.md");
        assert!(ms.exists());
    }

    #[test]
    fn rejects_empty_name() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        assert!(matches!(
            store.create("   "),
            Err(QuillError::InvalidArgument(_))
        ));
    }

    #[test]
    fn fresh_project_has_no_vault() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let p = store.create("Eragon").unwrap();
        assert!(p.vault_path.is_none());
        assert!(!p.vault_auto_watch);
    }

    #[test]
    fn update_sets_and_clears_vault_path() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let p = store.create("Wingfeather").unwrap();
        let id = p.id.clone();

        // Set
        let patch = ProjectPatch {
            vault_path: Some(Some("/Users/me/vault".into())),
            vault_auto_watch: Some(true),
            ..Default::default()
        };
        let p2 = store.update(&id, patch).unwrap();
        assert_eq!(p2.vault_path.as_deref(), Some("/Users/me/vault"));
        assert!(p2.vault_auto_watch);

        // Reload to ensure persisted
        let p3 = store.open(&id).unwrap();
        assert_eq!(p3.vault_path.as_deref(), Some("/Users/me/vault"));

        // Clear with Some(None)
        let patch = ProjectPatch {
            vault_path: Some(None),
            ..Default::default()
        };
        let p4 = store.update(&id, patch).unwrap();
        assert!(p4.vault_path.is_none());
    }

    #[test]
    fn old_project_json_without_vault_fields_loads() {
        // Simulate a project.json written by an earlier version that didn't
        // know about vault_path / vault_auto_watch.
        let dir = tempfile::tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let id = "legacy-id";
        let project_dir = dir.path().join(PROJECTS_DIR).join(id);
        std::fs::create_dir_all(&project_dir).unwrap();
        let legacy_json = serde_json::json!({
            "id": id,
            "name": "Legacy",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z",
            "manuscript_word_count": 0,
            "beat_progress": 0,
        });
        std::fs::write(
            project_dir.join(PROJECT_FILE),
            serde_json::to_string(&legacy_json).unwrap(),
        )
        .unwrap();

        let loaded = store.open(id).unwrap();
        assert_eq!(loaded.name, "Legacy");
        assert!(loaded.vault_path.is_none());
        assert!(!loaded.vault_auto_watch);
    }
}
