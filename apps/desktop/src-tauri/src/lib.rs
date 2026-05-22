//! Quill — Rust core library entry point.
//!
//! See `docs/ARCHITECTURE.md` for module boundaries.

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
            commands::settings_get,
            commands::settings_update,
            commands::theme_set,
            commands::secret_set,
            commands::secret_get,
            commands::secret_has,
            commands::git_commit,
            commands::git_log,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
