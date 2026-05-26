//! Quill — Rust core library entry point.
//!
//! See `docs/ARCHITECTURE.md` for module boundaries.

// Many APIs are scaffolded for upcoming phases (chat provider trait, vault
// watcher, beat helpers, etc.) and aren't called from the current command
// surface. We accept dead_code at the crate level rather than peppering
// individual `#[allow]` annotations across the codebase.
#![allow(dead_code)]

mod commands;
mod config;
mod error;
mod models;
mod services;
mod state;
mod telemetry;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    telemetry::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            // Resolve the app data directory. In dev, honor QUILL_DATA_DIR for isolation.
            let data_dir = config::resolve_data_dir(app.handle())?;
            tracing::info!(path = %data_dir.display(), "data directory resolved");

            // Initialize core services. Failures here are fatal: the app cannot
            // function without storage, so we propagate.
            let state = state::AppState::initialize(data_dir)?;
            app.manage(state);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::project_create,
            commands::project_list,
            commands::project_open,
            commands::project_update,
            commands::settings_get,
            commands::settings_update,
            commands::theme_set,
            commands::secret_set,
            commands::secret_get,
            commands::secret_has,
            commands::git_commit,
            commands::git_log,
            commands::canon_ingest_file,
            commands::canon_search,
            commands::canon_count,
            commands::canon_watch_start,
            commands::canon_watch_stop,
            commands::canon_watch_status,
            commands::llm_provider_status,
            commands::llm_ping,
            commands::audit_tail,
            commands::audit_path,
            commands::structure_beat_sheet_get,
            commands::structure_beat_update,
            commands::structure_beat_sheet_set_target,
            commands::structure_beat_sheet_set_frozen,
            commands::structure_outline_preview,
            commands::structure_outline_apply,
            commands::structure_scenes_list,
            commands::structure_scene_create,
            commands::structure_scene_delete,
            commands::structure_scene_reorder,
            commands::structure_scene_update,
            commands::manuscript_load_scene,
            commands::manuscript_save_scene,
            commands::voice_pins_list,
            commands::voice_pins_create,
            commands::voice_pins_delete,
            commands::voice_pins_update,
            commands::voice_fingerprint,
            commands::voice_extract,
            commands::voice_drift,
            commands::drafting_preview,
            commands::drafting_invoke,
            commands::brain_characters_list,
            commands::brain_character_create,
            commands::brain_character_update,
            commands::brain_character_delete,
            commands::brain_character_cross_links,
            commands::brain_ideas_list,
            commands::brain_idea_create,
            commands::brain_idea_update,
            commands::brain_idea_delete,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
