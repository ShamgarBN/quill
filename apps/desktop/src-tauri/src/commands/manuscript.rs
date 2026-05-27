//! Manuscript content commands.
//!
//! Bridges the structural-engine scene metadata (title, order, status) and
//! the on-disk Markdown content. The UI calls these on every editor flush.

use crate::error::{QuillError, Result};
use crate::models::structure::Scene;
use crate::services::git::GitService;
use crate::services::manuscript::{
    CompileOptions, CompileReport, ManuscriptStore, ProgressService, SceneContent, SearchHit,
    TodayProgress,
};
use crate::services::structure::StructureStore;
use crate::state::AppState;
use std::path::PathBuf;
use tauri::State;

/// Load a scene's prose. Resolves the scene's current order from the
/// structure store so the UI doesn't need to track it.
#[tauri::command]
pub fn manuscript_load_scene(
    state: State<'_, AppState>,
    project_id: String,
    scene_id: String,
) -> Result<SceneContent> {
    let scene = find_scene(&state, &project_id, &scene_id)?;
    let store = ManuscriptStore::new(&state.projects);
    store.load_scene(&project_id, &scene.id, scene.order)
}

/// Save a scene's prose and refresh the cached word count on the scene
/// metadata. Returns the updated content snapshot.
#[tauri::command]
pub fn manuscript_save_scene(
    state: State<'_, AppState>,
    project_id: String,
    scene_id: String,
    text: String,
) -> Result<SceneContent> {
    let scene = find_scene(&state, &project_id, &scene_id)?;
    let manuscript = ManuscriptStore::new(&state.projects);
    let saved = manuscript.save_scene(&project_id, &scene.id, scene.order, &text)?;

    // Mirror the word count back into the scene metadata so beat-sheet
    // progress views stay in sync without re-reading the file.
    let cached_count = saved.word_count;
    let structure = StructureStore::new(&state.projects);
    let _ = structure.update_scene(&project_id, &scene.id, |s| {
        s.word_count = cached_count;
    })?;

    // Auto-commit. We deliberately swallow git errors here: a save that
    // succeeded on disk should never appear to fail because the version
    // history append failed. The error is logged for triage.
    if let Ok(dir) = state.projects.root_dir(&project_id) {
        let title = scene.title.replace('\n', " ");
        let truncated = if title.len() > 80 {
            let mut t = title.chars().take(80).collect::<String>();
            t.push('…');
            t
        } else {
            title
        };
        let message = format!("draft: {truncated} ({cached_count} words)");
        let git = GitService::for_project(&dir);
        if let Err(e) = git.commit_all(Some(&message)) {
            tracing::warn!(error = %crate::error::DisplayErr(&e), "auto-commit failed");
        }
    }

    Ok(saved)
}

fn find_scene(state: &AppState, project_id: &str, scene_id: &str) -> Result<Scene> {
    let structure = StructureStore::new(&state.projects);
    let scenes = structure.load_scenes(project_id)?;
    scenes
        .into_iter()
        .find(|s| s.id == scene_id)
        .ok_or_else(|| QuillError::NotFound(format!("scene {scene_id}")))
}

/// Compile every scene's prose, in narrative order, into one Markdown
/// stream. If `output_path` is supplied, the compiled text is also
/// written to that file (atomically — partial writes won't corrupt
/// existing content).
#[tauri::command]
pub fn manuscript_compile(
    state: State<'_, AppState>,
    project_id: String,
    output_path: Option<String>,
    options: Option<CompileOptions>,
) -> Result<CompileReport> {
    let structure = StructureStore::new(&state.projects);
    let scenes = structure.load_scenes(&project_id)?;
    let manuscript = ManuscriptStore::new(&state.projects);
    let opts = options.unwrap_or_default();
    let path_buf = output_path.map(PathBuf::from);
    manuscript.compile(&project_id, &scenes, &opts, path_buf.as_deref())
}

/// Case-insensitive substring search across every scene's prose.
/// Default limit is 100 hits; the caller can override.
#[tauri::command]
pub fn manuscript_search(
    state: State<'_, AppState>,
    project_id: String,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<SearchHit>> {
    let structure = StructureStore::new(&state.projects);
    let scenes = structure.load_scenes(&project_id)?;
    let manuscript = ManuscriptStore::new(&state.projects);
    manuscript.search(&project_id, &scenes, &query, limit.unwrap_or(100))
}

/// Return today's writing progress for a project. First call of any given
/// day baselines today's count at the current total, so subsequent calls
/// can report a sensible delta.
#[tauri::command]
pub fn manuscript_today_progress(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<TodayProgress> {
    let structure = StructureStore::new(&state.projects);
    let scenes = structure.load_scenes(&project_id)?;
    let total: u64 = scenes.iter().map(|s| s.word_count as u64).sum();
    let svc = ProgressService::new(&state.projects);
    svc.today(&project_id, total)
}
