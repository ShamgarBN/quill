//! On-disk per-scene Markdown content store.

use crate::error::{QuillError, Result};
use crate::services::storage::{self, ProjectStore};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use unicode_segmentation::UnicodeSegmentation;

/// Returned to the UI when loading a scene file. The text is the raw,
/// untransformed Markdown body — no front-matter rewriting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneContent {
    pub scene_id: String,
    /// Always the absolute path so the UI can show "where on disk" and pass
    /// the value back if it ever wants to reveal the file in Finder.
    pub path: String,
    pub text: String,
    pub word_count: u32,
    pub char_count: u32,
}

pub struct ManuscriptStore<'a> {
    pub projects: &'a ProjectStore,
}

impl<'a> ManuscriptStore<'a> {
    pub fn new(projects: &'a ProjectStore) -> Self {
        Self { projects }
    }

    fn manuscript_dir(&self, project_id: &str) -> Result<PathBuf> {
        let root = self.projects.root_dir(project_id)?;
        let dir = root.join("manuscript");
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// Build the canonical scene file path: `<NNNN>-<scene-id>.md`.
    fn scene_file(&self, project_id: &str, order: u32, scene_id: &str) -> Result<PathBuf> {
        // Defensive: scene IDs are server-generated UUIDs so they should
        // already be filesystem-safe, but reject anything weird here so a
        // bad id can never traverse the manuscript directory.
        if !scene_id_is_safe(scene_id) {
            return Err(QuillError::InvalidArgument(format!(
                "unsafe scene id: {scene_id}"
            )));
        }
        Ok(self
            .manuscript_dir(project_id)?
            .join(format!("{order:04}-{scene_id}.md")))
    }

    /// Locate an existing scene file by scene id regardless of its current
    /// order prefix. Used when scenes get reordered between sessions.
    fn find_existing(&self, project_id: &str, scene_id: &str) -> Result<Option<PathBuf>> {
        if !scene_id_is_safe(scene_id) {
            return Err(QuillError::InvalidArgument(format!(
                "unsafe scene id: {scene_id}"
            )));
        }
        let dir = self.manuscript_dir(project_id)?;
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let p = entry.path();
            if !p.is_file() {
                continue;
            }
            let name = match p.file_name().and_then(|s| s.to_str()) {
                Some(n) => n,
                None => continue,
            };
            // Match `NNNN-<scene_id>.md`
            let suffix = format!("-{scene_id}.md");
            if name.ends_with(&suffix) {
                return Ok(Some(p));
            }
        }
        Ok(None)
    }

    /// Load the scene's prose. If the file doesn't exist yet, return an
    /// empty draft — a brand-new scene starts blank.
    pub fn load_scene(&self, project_id: &str, scene_id: &str, order: u32) -> Result<SceneContent> {
        let canonical = self.scene_file(project_id, order, scene_id)?;
        let path = if canonical.exists() {
            canonical
        } else {
            // Try to recover a file that's still under an old order prefix.
            match self.find_existing(project_id, scene_id)? {
                Some(p) => {
                    // Rename it into the new canonical position so downstream
                    // tools see one stable path per scene.
                    if p != canonical {
                        if let Some(parent) = canonical.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        std::fs::rename(&p, &canonical)?;
                        canonical
                    } else {
                        p
                    }
                }
                None => {
                    // Fresh scene: return an empty content object without
                    // writing anything to disk. We only commit a file once
                    // the user actually types something.
                    return Ok(SceneContent {
                        scene_id: scene_id.to_string(),
                        path: canonical.to_string_lossy().to_string(),
                        text: String::new(),
                        word_count: 0,
                        char_count: 0,
                    });
                }
            }
        };

        let bytes = std::fs::read(&path)?;
        let text = String::from_utf8(bytes)
            .map_err(|_| QuillError::Storage("scene file is not valid UTF-8".into()))?;
        Ok(SceneContent {
            scene_id: scene_id.to_string(),
            path: path.to_string_lossy().to_string(),
            text: text.clone(),
            word_count: count_words(&text),
            char_count: text.chars().count() as u32,
        })
    }

    /// Save the scene's prose. Returns the canonical content (with refreshed
    /// counts) so the UI can update its status pill in one round-trip.
    pub fn save_scene(
        &self,
        project_id: &str,
        scene_id: &str,
        order: u32,
        text: &str,
    ) -> Result<SceneContent> {
        let path = self.scene_file(project_id, order, scene_id)?;

        // If a file exists under a stale order prefix, remove it after the
        // new write succeeds. (Doing it after means a crash during save
        // never leaves the user with zero copies.)
        let stale = match self.find_existing(project_id, scene_id)? {
            Some(p) if p != path => Some(p),
            _ => None,
        };

        storage::atomic_write_bytes(&path, text.as_bytes())?;

        if let Some(stale) = stale {
            // Best-effort cleanup; a leftover file doesn't corrupt anything,
            // it just means the user briefly sees two copies in `ls`.
            let _ = std::fs::remove_file(stale);
        }

        Ok(SceneContent {
            scene_id: scene_id.to_string(),
            path: path.to_string_lossy().to_string(),
            text: text.to_string(),
            word_count: count_words(text),
            char_count: text.chars().count() as u32,
        })
    }

    /// Delete the on-disk file for a scene, if any. Used when a scene is
    /// removed from the structure store.
    pub fn delete_scene(&self, project_id: &str, scene_id: &str) -> Result<()> {
        if let Some(p) = self.find_existing(project_id, scene_id)? {
            std::fs::remove_file(p)?;
        }
        Ok(())
    }

    /// Concatenate every scene in narrative order into one Markdown stream
    /// and (optionally) write it to disk.
    ///
    /// `scenes` must already be in the order the user wants — the caller
    /// is responsible for that (typically `StructureStore::load_scenes`).
    /// The returned `CompileReport` carries the full text, the word count,
    /// and the scene count. If `output_path` is `Some`, the text is also
    /// written atomically to that path.
    pub fn compile(
        &self,
        project_id: &str,
        scenes: &[crate::models::structure::Scene],
        options: &CompileOptions,
        output_path: Option<&std::path::Path>,
    ) -> Result<CompileReport> {
        let mut out = String::new();
        let mut emitted = 0u32;
        for scene in scenes {
            let content = self.load_scene(project_id, &scene.id, scene.order)?;
            let trimmed = content.text.trim();
            if trimmed.is_empty() && !options.include_empty_scenes {
                continue;
            }
            if !out.is_empty() {
                out.push_str(&options.separator);
            }
            if options.include_scene_titles {
                out.push_str("## ");
                out.push_str(&scene.title);
                out.push_str("\n\n");
            }
            out.push_str(trimmed);
            emitted += 1;
        }
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        let word_count = count_words(&out);
        if let Some(path) = output_path {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            storage::atomic_write_bytes(path, out.as_bytes())?;
        }
        Ok(CompileReport {
            markdown: out,
            word_count,
            scene_count: emitted,
            output_path: output_path.map(|p| p.to_string_lossy().to_string()),
        })
    }
}

/// Options for `ManuscriptStore::compile`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CompileOptions {
    /// Render each scene's title as a `## ...` H2 above its prose.
    pub include_scene_titles: bool,
    /// If false (default), scenes whose prose is empty are skipped.
    pub include_empty_scenes: bool,
    /// What to insert between consecutive scenes. Default `\n\n`.
    pub separator: String,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            include_scene_titles: false,
            include_empty_scenes: false,
            separator: "\n\n".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CompileReport {
    pub markdown: String,
    pub word_count: u32,
    pub scene_count: u32,
    pub output_path: Option<String>,
}

fn scene_id_is_safe(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Word counter that treats words as Unicode word boundaries — handles
/// punctuation, em-dashes, and curly quotes the way a human reader would.
fn count_words(text: &str) -> u32 {
    text.unicode_words().count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::storage::ProjectStore;

    fn fixture() -> (tempfile::TempDir, ProjectStore, String) {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let project = projects.create("Demo").unwrap();
        let id = project.id;
        (dir, projects, id)
    }

    #[test]
    fn load_returns_empty_for_new_scene() {
        let (_d, ps, pid) = fixture();
        let store = ManuscriptStore::new(&ps);
        let c = store.load_scene(&pid, "scn_aaaa", 0).unwrap();
        assert_eq!(c.text, "");
        assert_eq!(c.word_count, 0);
    }

    #[test]
    fn save_then_load_roundtrips() {
        let (_d, ps, pid) = fixture();
        let store = ManuscriptStore::new(&ps);
        let s1 = store
            .save_scene(&pid, "scn_aaaa", 0, "The dragon flew over the lake.")
            .unwrap();
        assert_eq!(s1.word_count, 6);
        let s2 = store.load_scene(&pid, "scn_aaaa", 0).unwrap();
        assert_eq!(s2.text, "The dragon flew over the lake.");
        assert_eq!(s2.word_count, 6);
    }

    #[test]
    fn rename_follows_order_change() {
        let (_d, ps, pid) = fixture();
        let store = ManuscriptStore::new(&ps);
        store.save_scene(&pid, "scn_aaaa", 2, "Hello.").unwrap();
        // Now load at a different order — the file should be relocated.
        let c = store.load_scene(&pid, "scn_aaaa", 7).unwrap();
        assert_eq!(c.text, "Hello.");
        assert!(c.path.contains("0007-scn_aaaa.md"));
    }

    #[test]
    fn rejects_unsafe_scene_id() {
        let (_d, ps, pid) = fixture();
        let store = ManuscriptStore::new(&ps);
        let r = store.save_scene(&pid, "../etc/passwd", 0, "x");
        assert!(matches!(r, Err(QuillError::InvalidArgument(_))));
    }

    #[test]
    fn delete_removes_file_if_present() {
        let (_d, ps, pid) = fixture();
        let store = ManuscriptStore::new(&ps);
        store.save_scene(&pid, "scn_aaaa", 0, "x").unwrap();
        store.delete_scene(&pid, "scn_aaaa").unwrap();
        let c = store.load_scene(&pid, "scn_aaaa", 0).unwrap();
        assert_eq!(c.text, "");
    }

    fn scene(order: u32, id: &str, title: &str) -> crate::models::structure::Scene {
        use chrono::Utc;
        crate::models::structure::Scene {
            id: id.to_string(),
            project_id: "p".into(),
            order,
            title: title.into(),
            pov: None,
            setting: None,
            status: crate::models::structure::SceneStatus::Drafting,
            word_count: 0,
            beat_id: None,
            inciting_incident: String::new(),
            progressive_complication: String::new(),
            crisis: String::new(),
            climax: String::new(),
            resolution: String::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn compile_joins_scenes_in_order() {
        let (_d, ps, pid) = fixture();
        let store = ManuscriptStore::new(&ps);
        store
            .save_scene(&pid, "scn_a", 0, "First scene prose.")
            .unwrap();
        store
            .save_scene(&pid, "scn_b", 1, "Second scene prose.")
            .unwrap();
        store
            .save_scene(&pid, "scn_c", 2, "Third scene prose.")
            .unwrap();

        let scenes = vec![
            scene(0, "scn_a", "Opening"),
            scene(1, "scn_b", "Middle"),
            scene(2, "scn_c", "End"),
        ];
        let report = store
            .compile(&pid, &scenes, &CompileOptions::default(), None)
            .unwrap();
        assert_eq!(report.scene_count, 3);
        assert_eq!(
            report.markdown,
            "First scene prose.\n\nSecond scene prose.\n\nThird scene prose.\n"
        );
        assert_eq!(report.word_count, 9);
    }

    #[test]
    fn compile_skips_empty_scenes_by_default() {
        let (_d, ps, pid) = fixture();
        let store = ManuscriptStore::new(&ps);
        store.save_scene(&pid, "scn_a", 0, "Real prose.").unwrap();
        // scn_b is never saved → empty.
        store.save_scene(&pid, "scn_c", 2, "More prose.").unwrap();

        let scenes = vec![
            scene(0, "scn_a", "A"),
            scene(1, "scn_b", "B"),
            scene(2, "scn_c", "C"),
        ];
        let report = store
            .compile(&pid, &scenes, &CompileOptions::default(), None)
            .unwrap();
        assert_eq!(report.scene_count, 2);
        assert_eq!(report.markdown, "Real prose.\n\nMore prose.\n");
    }

    #[test]
    fn compile_with_titles_emits_h2_headings() {
        let (_d, ps, pid) = fixture();
        let store = ManuscriptStore::new(&ps);
        store.save_scene(&pid, "scn_a", 0, "Body one.").unwrap();
        store.save_scene(&pid, "scn_b", 1, "Body two.").unwrap();
        let scenes = vec![
            scene(0, "scn_a", "The Beginning"),
            scene(1, "scn_b", "The Middle"),
        ];
        let report = store
            .compile(
                &pid,
                &scenes,
                &CompileOptions {
                    include_scene_titles: true,
                    ..CompileOptions::default()
                },
                None,
            )
            .unwrap();
        assert!(report.markdown.contains("## The Beginning\n\nBody one."));
        assert!(report.markdown.contains("## The Middle\n\nBody two."));
    }

    #[test]
    fn compile_writes_to_disk_when_path_supplied() {
        let (d, ps, pid) = fixture();
        let store = ManuscriptStore::new(&ps);
        store.save_scene(&pid, "scn_a", 0, "Only scene.").unwrap();
        let out = d.path().join("compiled.md");
        let report = store
            .compile(
                &pid,
                &[scene(0, "scn_a", "Only")],
                &CompileOptions::default(),
                Some(&out),
            )
            .unwrap();
        assert_eq!(
            report.output_path.as_deref(),
            Some(out.to_string_lossy().as_ref())
        );
        let on_disk = std::fs::read_to_string(&out).unwrap();
        assert_eq!(on_disk, "Only scene.\n");
    }
}
