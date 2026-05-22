//! Tauri command handlers.
//!
//! Discipline: handlers are thin glue. They:
//! 1. Accept typed input
//! 2. Delegate to a service
//! 3. Return a typed result
//!
//! Zero business logic in this layer — it lives in `services::*`.

mod canon;
mod llm;
mod manuscript;
mod structure;
mod voice;

pub use canon::*;
pub use llm::*;
pub use manuscript::*;
pub use structure::*;
pub use voice::*;

use crate::config;
use crate::error::Result;
use crate::models::settings::SettingsPatch;
use crate::models::{CommitInfo, Project, Settings};
use crate::services::git::GitService;
use crate::state::AppState;
use serde::Serialize;
use tauri::State;

// ---------- App info ----------

#[derive(Serialize)]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
    pub phase: String,
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> Result<AppInfo> {
    Ok(AppInfo {
        version: config::APP_VERSION.to_string(),
        data_dir: state.data_dir.display().to_string(),
        phase: config::APP_PHASE.to_string(),
    })
}

// ---------- Projects ----------

#[tauri::command]
pub fn project_create(state: State<'_, AppState>, name: String) -> Result<Project> {
    let project = state.projects.create(&name)?;
    let dir = state.projects.root_dir(&project.id)?;
    let git = GitService::for_project(&dir);
    let _ = git.commit_all(Some("initial: project created"))?;
    Ok(project)
}

#[tauri::command]
pub fn project_list(state: State<'_, AppState>) -> Result<Vec<Project>> {
    state.projects.list()
}

#[tauri::command]
pub fn project_open(state: State<'_, AppState>, id: String) -> Result<Project> {
    state.projects.open(&id)
}

// ---------- Settings ----------

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> Result<Settings> {
    let _g = state.settings_lock.lock().expect("settings lock poisoned");
    state.settings_store.load_or_init()
}

#[tauri::command]
pub fn settings_update(state: State<'_, AppState>, patch: SettingsPatch) -> Result<Settings> {
    let _g = state.settings_lock.lock().expect("settings lock poisoned");
    let mut current = state.settings_store.load_or_init()?;
    patch.apply(&mut current);
    state.settings_store.save(&current)?;
    Ok(current)
}

#[tauri::command]
pub fn theme_set(state: State<'_, AppState>, theme: crate::models::ThemePreference) -> Result<()> {
    let _g = state.settings_lock.lock().expect("settings lock poisoned");
    let mut s = state.settings_store.load_or_init()?;
    s.theme = theme;
    state.settings_store.save(&s)?;
    Ok(())
}

// ---------- Secrets ----------

#[tauri::command]
pub fn secret_set(state: State<'_, AppState>, key: String, value: String) -> Result<()> {
    state.secrets.set(&key, &value)
}

#[tauri::command]
pub fn secret_get(state: State<'_, AppState>, key: String) -> Result<Option<String>> {
    state.secrets.get(&key)
}

#[tauri::command]
pub fn secret_has(state: State<'_, AppState>, key: String) -> Result<bool> {
    Ok(state.secrets.has(&key))
}

// ---------- Git ----------

#[tauri::command]
pub fn git_commit(
    state: State<'_, AppState>,
    project_id: String,
    message: Option<String>,
) -> Result<CommitInfo> {
    let dir = state.projects.root_dir(&project_id)?;
    let git = GitService::for_project(&dir);
    let info = git.commit_all(message.as_deref())?;
    info.ok_or_else(|| crate::error::QuillError::Storage("nothing to commit".into()))
}

#[tauri::command]
pub fn git_log(
    state: State<'_, AppState>,
    project_id: String,
    limit: Option<usize>,
) -> Result<Vec<CommitInfo>> {
    let dir = state.projects.root_dir(&project_id)?;
    let git = GitService::for_project(&dir);
    git.log(limit.unwrap_or(20))
}
