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

    /// Case-insensitive substring search over every scene's prose. Returns
    /// up to `max_hits` matches with a ~80-char snippet around each match.
    ///
    /// `scenes` must already be in narrative order; the order is preserved
    /// in the result.
    pub fn search(
        &self,
        project_id: &str,
        scenes: &[crate::models::structure::Scene],
        query: &str,
        max_hits: usize,
    ) -> Result<Vec<SearchHit>> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let q_lower = q.to_lowercase();
        let mut hits = Vec::new();
        for scene in scenes {
            if hits.len() >= max_hits {
                break;
            }
            let content = match self.load_scene(project_id, &scene.id, scene.order) {
                Ok(c) => c,
                Err(_) => continue,
            };
            if content.text.is_empty() {
                continue;
            }
            let lower = content.text.to_lowercase();
            let mut start = 0;
            while let Some(rel) = lower[start..].find(&q_lower) {
                if hits.len() >= max_hits {
                    break;
                }
                let abs = start + rel;
                let line = 1 + content.text[..abs].matches('\n').count() as u32;
                let snippet = make_snippet(&content.text, abs, abs + q.len(), 60);
                hits.push(SearchHit {
                    scene_id: scene.id.clone(),
                    scene_title: scene.title.clone(),
                    scene_order: scene.order,
                    line,
                    snippet,
                    matched_text: content.text[abs..abs + q.len()].to_string(),
                });
                start = abs + q.len().max(1);
            }
        }
        Ok(hits)
    }

    /// Concatenate every scene in narrative order into one Markdown stream
    /// and (optionally) write it to disk.
    ///
    /// `scenes` must already be in the order the user wants — the caller
    /// is responsible for that (typically `StructureStore::load_scenes`).
    /// `chapters` (in chapter order) drives `# Chapter N` headings at
    /// chapter boundaries and the `only_chapter_id` filter; pass `&[]`
    /// for a headerless scene stream. The returned `CompileReport`
    /// carries the full text, the word count, and the scene count. If
    /// `output_path` is `Some`, the text is also written atomically.
    pub fn compile(
        &self,
        project_id: &str,
        scenes: &[crate::models::structure::Scene],
        chapters: &[crate::models::structure::Chapter],
        options: &CompileOptions,
        output_path: Option<&std::path::Path>,
    ) -> Result<CompileReport> {
        let mut out = String::new();
        let mut emitted = 0u32;
        let mut last_chapter: Option<String> = None;
        for scene in scenes {
            if let Some(only) = options.only_chapter_id.as_deref() {
                if scene.chapter_id.as_deref() != Some(only) {
                    continue;
                }
            }
            let content = self.load_scene(project_id, &scene.id, scene.order)?;
            let trimmed = content.text.trim();
            if trimmed.is_empty() && !options.include_empty_scenes {
                continue;
            }
            if !out.is_empty() {
                out.push_str(&options.separator);
            }
            // Chapter heading at each chapter boundary.
            if options.include_chapter_headings
                && !chapters.is_empty()
                && scene.chapter_id != last_chapter
            {
                if let Some(pos) = scene
                    .chapter_id
                    .as_deref()
                    .and_then(|c| chapters.iter().position(|ch| ch.id == c))
                {
                    let ch = &chapters[pos];
                    out.push_str(&format!("# Chapter {}", pos + 1));
                    let title = ch.title.trim();
                    if !title.is_empty() && title != format!("Chapter {}", pos + 1) {
                        out.push_str(&format!(" — {title}"));
                    }
                    out.push_str("\n\n");
                }
                last_chapter = scene.chapter_id.clone();
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
    /// Render `# Chapter N — Title` at each chapter boundary. Default
    /// true (no-op when the caller passes no chapters).
    pub include_chapter_headings: bool,
    /// Compile only the scenes of this chapter — per-chapter export.
    pub only_chapter_id: Option<String>,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            include_scene_titles: false,
            include_empty_scenes: false,
            separator: "\n\n".into(),
            include_chapter_headings: true,
            only_chapter_id: None,
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

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub scene_id: String,
    pub scene_title: String,
    pub scene_order: u32,
    /// 1-based line number where the match begins.
    pub line: u32,
    /// ~80-char excerpt around the match, with the matched text intact.
    pub snippet: String,
    /// The exact matched substring (preserves the original case of the
    /// scene text, not the query's case).
    pub matched_text: String,
}

/// Build a snippet centered on the match. Boundaries are word-aware: we
/// extend forward and backward from the match by ~`pad` characters, then
/// shrink to the nearest whitespace so the snippet doesn't cut mid-word.
fn make_snippet(text: &str, match_start: usize, match_end: usize, pad: usize) -> String {
    // Work in char indices so we don't slice in the middle of a UTF-8 code point.
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let to_char_idx = |b: usize| -> usize {
        chars
            .iter()
            .position(|(i, _)| *i >= b)
            .unwrap_or(chars.len())
    };
    let m_start_ci = to_char_idx(match_start);
    let m_end_ci = to_char_idx(match_end);
    let lo = m_start_ci.saturating_sub(pad);
    let hi = (m_end_ci + pad).min(chars.len());

    // Pull the byte offsets back out.
    let lo_b = chars.get(lo).map(|(b, _)| *b).unwrap_or(0);
    let hi_b = chars.get(hi).map(|(b, _)| *b).unwrap_or(text.len());
    let raw = &text[lo_b..hi_b];

    // Trim to nearest whitespace boundaries so we don't cut mid-word.
    let trimmed = trim_to_word_bounds(raw);
    let mut out = String::new();
    if lo_b > 0 {
        out.push('…');
    }
    // Collapse internal newlines to spaces so the snippet renders on one line.
    for c in trimmed.chars() {
        if c == '\n' || c == '\r' {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    if hi_b < text.len() {
        out.push('…');
    }
    out
}

fn trim_to_word_bounds(s: &str) -> &str {
    // Trim leading characters up to (but not including) the first whitespace
    // boundary if we're clearly mid-word. Mirror at the end.
    let bytes = s.as_bytes();
    let mut start = 0;
    if !bytes.is_empty() && !s.starts_with(char::is_whitespace) {
        // Walk forward to the first whitespace, then past it, but cap.
        if let Some((idx, _)) = s.char_indices().find(|(_, c)| c.is_whitespace()) {
            // Only trim if it's a small step — don't lose half the snippet.
            if idx <= 20 {
                start = idx + 1;
            }
        }
    }
    let mut end = s.len();
    if !s.ends_with(char::is_whitespace) {
        // Walk backward to the last whitespace, only if it's near the end.
        if let Some(idx) = s
            .char_indices()
            .rev()
            .find(|(_, c)| c.is_whitespace())
            .map(|(i, _)| i)
        {
            if s.len() - idx <= 20 {
                end = idx;
            }
        }
    }
    if start >= end {
        return s;
    }
    s[start..end].trim()
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
            thread_ids: Vec::new(),
            chapter_id: None,
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
            .compile(&pid, &scenes, &[], &CompileOptions::default(), None)
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
            .compile(&pid, &scenes, &[], &CompileOptions::default(), None)
            .unwrap();
        assert_eq!(report.scene_count, 2);
        assert_eq!(report.markdown, "Real prose.\n\nMore prose.\n");
    }

    #[test]
    fn compile_emits_chapter_headings_and_per_chapter_export() {
        use crate::models::structure::Chapter;
        let (_d, ps, pid) = fixture();
        let store = ManuscriptStore::new(&ps);
        store.save_scene(&pid, "scn_a", 0, "Alpha prose.").unwrap();
        store.save_scene(&pid, "scn_b", 1, "Beta prose.").unwrap();

        let ch1 = Chapter::fresh(&pid, 0, "Chapter 1");
        let mut ch2 = Chapter::fresh(&pid, 1, "");
        ch2.title = "The Drowned Chapel".into();
        let mut s1 = scene(0, "scn_a", "A");
        s1.chapter_id = Some(ch1.id.clone());
        let mut s2 = scene(1, "scn_b", "B");
        s2.chapter_id = Some(ch2.id.clone());
        let scenes = vec![s1, s2];
        let chapters = vec![ch1, ch2.clone()];

        let report = store
            .compile(&pid, &scenes, &chapters, &CompileOptions::default(), None)
            .unwrap();
        // Generic title "Chapter 1" isn't repeated after the number; a
        // real title is.
        assert!(report.markdown.starts_with("# Chapter 1\n\nAlpha prose."));
        assert!(report
            .markdown
            .contains("# Chapter 2 — The Drowned Chapel\n\nBeta prose."));

        // Headings off → plain stream.
        let report = store
            .compile(
                &pid,
                &scenes,
                &chapters,
                &CompileOptions {
                    include_chapter_headings: false,
                    ..CompileOptions::default()
                },
                None,
            )
            .unwrap();
        assert!(!report.markdown.contains("# Chapter"));

        // Per-chapter export: only chapter 2's scenes.
        let report = store
            .compile(
                &pid,
                &scenes,
                &chapters,
                &CompileOptions {
                    only_chapter_id: Some(ch2.id.clone()),
                    ..CompileOptions::default()
                },
                None,
            )
            .unwrap();
        assert_eq!(report.scene_count, 1);
        assert!(report.markdown.contains("Beta prose."));
        assert!(!report.markdown.contains("Alpha prose."));
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
                &[],
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
    fn search_returns_hits_per_scene_in_order() {
        let (_d, ps, pid) = fixture();
        let store = ManuscriptStore::new(&ps);
        store
            .save_scene(&pid, "scn_a", 0, "The dragon flew over the lake.")
            .unwrap();
        store
            .save_scene(&pid, "scn_b", 1, "No dragons here.")
            .unwrap();
        store
            .save_scene(&pid, "scn_c", 2, "Dragon. Dragon. Dragon.")
            .unwrap();

        let scenes = vec![
            scene(0, "scn_a", "A"),
            scene(1, "scn_b", "B"),
            scene(2, "scn_c", "C"),
        ];
        let hits = store.search(&pid, &scenes, "dragon", 100).unwrap();
        assert_eq!(hits.len(), 1 + 1 + 3);
        assert_eq!(hits[0].scene_id, "scn_a");
        assert_eq!(hits[1].scene_id, "scn_b");
        assert_eq!(hits.iter().filter(|h| h.scene_id == "scn_c").count(), 3);
        // All matched_text values preserve original casing.
        assert!(hits
            .iter()
            .all(|h| h.matched_text.to_lowercase() == "dragon"));
    }

    #[test]
    fn search_is_case_insensitive_and_respects_limit() {
        let (_d, ps, pid) = fixture();
        let store = ManuscriptStore::new(&ps);
        store
            .save_scene(&pid, "scn_a", 0, "DRAGON dragon Dragon DrAgOn")
            .unwrap();
        let scenes = vec![scene(0, "scn_a", "A")];
        let all = store.search(&pid, &scenes, "DRAGON", 100).unwrap();
        assert_eq!(all.len(), 4);
        let limited = store.search(&pid, &scenes, "dragon", 2).unwrap();
        assert_eq!(limited.len(), 2);
    }

    #[test]
    fn search_empty_query_returns_no_hits() {
        let (_d, ps, pid) = fixture();
        let store = ManuscriptStore::new(&ps);
        store.save_scene(&pid, "scn_a", 0, "anything").unwrap();
        let scenes = vec![scene(0, "scn_a", "A")];
        assert!(store.search(&pid, &scenes, "", 100).unwrap().is_empty());
        assert!(store.search(&pid, &scenes, "   ", 100).unwrap().is_empty());
    }

    #[test]
    fn search_snippet_contains_match() {
        let (_d, ps, pid) = fixture();
        let store = ManuscriptStore::new(&ps);
        store
            .save_scene(
                &pid,
                "scn_a",
                0,
                "Once upon a time there was a small village by the lake where dragons would gather every spring to drink the cold water.",
            )
            .unwrap();
        let scenes = vec![scene(0, "scn_a", "A")];
        let hits = store.search(&pid, &scenes, "dragons", 1).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(
            hits[0].snippet.to_lowercase().contains("dragons"),
            "snippet '{}' missing match",
            hits[0].snippet
        );
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
                &[],
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
