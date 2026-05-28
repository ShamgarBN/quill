//! Structural engine commands.

use crate::error::Result;
use crate::models::structure::{BeatId, BeatSheet, Scene};
use crate::services::structure::{parse_outline, ImportPreview, StructureStore};
use crate::state::AppState;
use serde::Deserialize;
use tauri::State;

#[tauri::command]
pub fn structure_beat_sheet_get(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<BeatSheet> {
    StructureStore::new(&state.projects).load_beat_sheet(&project_id)
}

#[derive(Deserialize)]
pub struct BeatPatch {
    pub summary: Option<String>,
    pub override_pct: Option<Option<f32>>,
    pub anchor_word: Option<Option<u32>>,
    pub satisfied: Option<bool>,
    pub locked: Option<bool>,
}

#[tauri::command]
pub fn structure_beat_update(
    state: State<'_, AppState>,
    project_id: String,
    beat_id: BeatId,
    patch: BeatPatch,
) -> Result<BeatSheet> {
    StructureStore::new(&state.projects).update_beat(&project_id, beat_id, |b| {
        if let Some(v) = patch.summary {
            b.summary = v;
        }
        if let Some(v) = patch.override_pct {
            b.override_pct = v;
        }
        if let Some(v) = patch.anchor_word {
            b.anchor_word = v;
        }
        if let Some(v) = patch.satisfied {
            b.satisfied = v;
        }
        if let Some(v) = patch.locked {
            b.locked = v;
        }
    })
}

#[tauri::command]
pub fn structure_beat_sheet_set_target(
    state: State<'_, AppState>,
    project_id: String,
    target_word_count: u32,
) -> Result<BeatSheet> {
    StructureStore::new(&state.projects).set_target_word_count(&project_id, target_word_count)
}

#[tauri::command]
pub fn structure_beat_sheet_set_frozen(
    state: State<'_, AppState>,
    project_id: String,
    frozen: bool,
) -> Result<BeatSheet> {
    StructureStore::new(&state.projects).set_frozen(&project_id, frozen)
}

#[tauri::command]
pub fn structure_outline_preview(text: String) -> Result<ImportPreview> {
    Ok(parse_outline(&text))
}

#[tauri::command]
pub fn structure_outline_apply(
    state: State<'_, AppState>,
    project_id: String,
    text: String,
) -> Result<BeatSheet> {
    let preview = parse_outline(&text);
    let store = StructureStore::new(&state.projects);
    let mut sheet = store.load_beat_sheet(&project_id)?;
    if sheet.frozen {
        return Err(crate::error::QuillError::InvalidArgument(
            "beat sheet is frozen; unfreeze first".into(),
        ));
    }
    for ib in preview.matched {
        if let Some(beat) = sheet.beats.iter_mut().find(|b| b.id == ib.id) {
            // Don't overwrite a locked beat.
            if !beat.locked {
                beat.summary = ib.summary;
            }
        }
    }
    sheet.updated_at = chrono::Utc::now();
    store.save_beat_sheet(&sheet)?;
    Ok(sheet)
}

// ---------- Scenes ----------

#[tauri::command]
pub fn structure_scenes_list(state: State<'_, AppState>, project_id: String) -> Result<Vec<Scene>> {
    StructureStore::new(&state.projects).load_scenes(&project_id)
}

#[tauri::command]
pub fn structure_scene_create(
    state: State<'_, AppState>,
    project_id: String,
    title: String,
    beat_id: Option<BeatId>,
) -> Result<Scene> {
    StructureStore::new(&state.projects).create_scene(&project_id, &title, beat_id)
}

#[tauri::command]
pub fn structure_scene_delete(
    state: State<'_, AppState>,
    project_id: String,
    scene_id: String,
) -> Result<()> {
    // Remove the metadata first; if the user re-creates the scene later
    // we don't want a leftover file under the same id to come back to
    // life as a "phantom" draft.
    StructureStore::new(&state.projects).delete_scene(&project_id, &scene_id)?;
    crate::services::manuscript::ManuscriptStore::new(&state.projects)
        .delete_scene(&project_id, &scene_id)?;
    Ok(())
}

#[tauri::command]
pub fn structure_scene_reorder(
    state: State<'_, AppState>,
    project_id: String,
    ids_in_order: Vec<String>,
) -> Result<()> {
    StructureStore::new(&state.projects).reorder_scenes(&project_id, &ids_in_order)
}

#[derive(Deserialize)]
pub struct ScenePatch {
    pub title: Option<String>,
    pub pov: Option<Option<String>>,
    pub setting: Option<Option<String>>,
    pub status: Option<crate::models::structure::SceneStatus>,
    pub word_count: Option<u32>,
    pub beat_id: Option<Option<BeatId>>,
    pub inciting_incident: Option<String>,
    pub progressive_complication: Option<String>,
    pub crisis: Option<String>,
    pub climax: Option<String>,
    pub resolution: Option<String>,
    /// Wholesale-replace the scene's linked thread ids. Pass `Some(Vec::new())`
    /// to clear, `Some(vec![...])` to set, omit to leave alone.
    pub thread_ids: Option<Vec<String>>,
}

#[tauri::command]
pub fn structure_scene_update(
    state: State<'_, AppState>,
    project_id: String,
    scene_id: String,
    patch: ScenePatch,
) -> Result<Scene> {
    StructureStore::new(&state.projects).update_scene(&project_id, &scene_id, |s| {
        if let Some(v) = patch.title {
            s.title = v;
        }
        if let Some(v) = patch.pov {
            s.pov = v;
        }
        if let Some(v) = patch.setting {
            s.setting = v;
        }
        if let Some(v) = patch.status {
            s.status = v;
        }
        if let Some(v) = patch.word_count {
            s.word_count = v;
        }
        if let Some(v) = patch.beat_id {
            s.beat_id = v;
        }
        if let Some(v) = patch.inciting_incident {
            s.inciting_incident = v;
        }
        if let Some(v) = patch.progressive_complication {
            s.progressive_complication = v;
        }
        if let Some(v) = patch.crisis {
            s.crisis = v;
        }
        if let Some(v) = patch.climax {
            s.climax = v;
        }
        if let Some(v) = patch.resolution {
            s.resolution = v;
        }
        if let Some(v) = patch.thread_ids {
            s.thread_ids = v;
        }
    })
}
