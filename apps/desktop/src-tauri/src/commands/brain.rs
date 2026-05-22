//! Phase 7 — Character Bible + Idea Park commands.

use crate::error::{QuillError, Result};
use crate::models::brain::{Character, CharacterPatch, CrossLink, Idea, IdeaPatch};
use crate::services::brain::{find_cross_links, CharacterStore, IdeaStore};
use crate::state::AppState;
use tauri::State;

// ---------- Characters ----------

#[tauri::command]
pub fn brain_characters_list(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<Character>> {
    CharacterStore::new(&state.projects).list(&project_id)
}

#[tauri::command]
pub fn brain_character_create(
    state: State<'_, AppState>,
    project_id: String,
    name: String,
) -> Result<Character> {
    CharacterStore::new(&state.projects).create(&project_id, &name)
}

#[tauri::command]
pub fn brain_character_update(
    state: State<'_, AppState>,
    project_id: String,
    id: String,
    patch: CharacterPatch,
) -> Result<Character> {
    CharacterStore::new(&state.projects).update(&project_id, &id, patch)
}

#[tauri::command]
pub fn brain_character_delete(
    state: State<'_, AppState>,
    project_id: String,
    id: String,
) -> Result<()> {
    CharacterStore::new(&state.projects).delete(&project_id, &id)
}

#[tauri::command]
pub async fn brain_character_cross_links(
    state: State<'_, AppState>,
    project_id: String,
    id: String,
) -> Result<Vec<CrossLink>> {
    let store = CharacterStore::new(&state.projects);
    let chars = store.list(&project_id)?;
    let character = chars
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| QuillError::NotFound(format!("character {id}")))?;
    find_cross_links(&character, &state.projects, state.vectors.as_ref()).await
}

// ---------- Ideas ----------

#[tauri::command]
pub fn brain_ideas_list(state: State<'_, AppState>, project_id: String) -> Result<Vec<Idea>> {
    IdeaStore::new(&state.projects).list(&project_id)
}

#[tauri::command]
pub fn brain_idea_create(
    state: State<'_, AppState>,
    project_id: String,
    text: String,
) -> Result<Idea> {
    IdeaStore::new(&state.projects).create(&project_id, &text)
}

#[tauri::command]
pub fn brain_idea_update(
    state: State<'_, AppState>,
    project_id: String,
    id: String,
    patch: IdeaPatch,
) -> Result<Idea> {
    IdeaStore::new(&state.projects).update(&project_id, &id, patch)
}

#[tauri::command]
pub fn brain_idea_delete(state: State<'_, AppState>, project_id: String, id: String) -> Result<()> {
    IdeaStore::new(&state.projects).delete(&project_id, &id)
}
