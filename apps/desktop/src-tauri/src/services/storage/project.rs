//! Per-project filesystem layout and CRUD.

use crate::error::{QuillError, Result};
use crate::models::Project;
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
}
