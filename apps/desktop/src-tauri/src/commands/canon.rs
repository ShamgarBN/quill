//! Canon ingestion + retrieval commands.

use crate::error::{QuillError, Result};
use crate::models::{CanonKind, ChunkRef, ChunkSensitivity};
use crate::services::canon::{IngestReport, IngestService, WatchStatus};
use crate::services::llm::ProviderId;
use crate::state::AppState;
use std::path::{Path, PathBuf};
use tauri::State;

/// Resolve the configured embeddings provider from settings.
async fn embedder_for(
    state: &AppState,
) -> Result<std::sync::Arc<dyn crate::services::llm::EmbeddingsProvider>> {
    let settings = state.settings_store.load_or_init()?;
    let provider_id = settings.embedding_provider;
    match provider_id {
        ProviderId::Mock => state.providers.embeddings(ProviderId::Mock),
        ProviderId::Gemini => state.providers.embeddings(ProviderId::Gemini),
        ProviderId::Groq => Err(QuillError::InvalidArgument(
            "Groq does not provide embeddings; choose Gemini or Mock".into(),
        )),
    }
}

#[tauri::command]
pub async fn canon_ingest_file(
    state: State<'_, AppState>,
    project_id: String,
    path: String,
    kind: Option<CanonKind>,
    sensitivity: Option<ChunkSensitivity>,
) -> Result<IngestReport> {
    let path_buf = PathBuf::from(&path);
    let embedder = embedder_for(&state).await?;
    let svc = IngestService::new(&*embedder, &*state.vectors);
    svc.ingest_file(
        &project_id,
        &path_buf,
        kind,
        sensitivity.unwrap_or(ChunkSensitivity::Public),
    )
    .await
}

#[tauri::command]
pub async fn canon_search(
    state: State<'_, AppState>,
    project_id: String,
    query: String,
    k: Option<usize>,
    respect_do_not_send: Option<bool>,
) -> Result<Vec<ChunkRef>> {
    let embedder = embedder_for(&state).await?;
    let svc = IngestService::new(&*embedder, &*state.vectors);
    svc.retrieve(
        &project_id,
        &query,
        k.unwrap_or(5),
        respect_do_not_send.unwrap_or(true),
    )
    .await
}

#[tauri::command]
pub async fn canon_count(state: State<'_, AppState>, project_id: String) -> Result<u64> {
    state.vectors.count_for_project(&project_id).await
}

// ---------- Vault watcher (Phase 5.x) ----------

/// Start watching the project's configured vault directory.
///
/// Resolution: if `vault_path` is supplied, use it (and persist it to the
/// project so the next start-up call uses the same value). Otherwise fall
/// back to the project's persisted `vault_path`. Errors if neither is set.
#[tauri::command]
pub async fn canon_watch_start(
    state: State<'_, AppState>,
    project_id: String,
    vault_path: Option<String>,
) -> Result<WatchStatus> {
    // Resolve the path: prefer the request arg, then the persisted value.
    let project = state.projects.open(&project_id)?;
    let path_str = vault_path
        .or(project.vault_path.clone())
        .ok_or_else(|| QuillError::InvalidArgument("no vault_path set for project".into()))?;

    let path = Path::new(&path_str).to_path_buf();
    let embedder = embedder_for(&state).await?;
    let vectors = state.vectors.clone();

    let status = state
        .watches
        .start(&project_id, &path, embedder, vectors)
        .await?;

    // Persist the path + enable the auto-watch flag so the next app start
    // can rehydrate this state if we add boot-time auto-resume.
    let patch = crate::models::ProjectPatch {
        vault_path: Some(Some(path_str)),
        vault_auto_watch: Some(true),
        ..Default::default()
    };
    let _ = state.projects.update(&project_id, patch)?;

    Ok(status)
}

/// Stop the active watch for a project. Returns the post-stop status (which
/// is `None` if no watch was running).
#[tauri::command]
pub async fn canon_watch_stop(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Option<WatchStatus>> {
    let _ = state.watches.stop(&project_id).await;
    // Clear the auto-watch flag so we don't auto-resume next boot.
    let patch = crate::models::ProjectPatch {
        vault_auto_watch: Some(false),
        ..Default::default()
    };
    let _ = state.projects.update(&project_id, patch)?;
    Ok(state.watches.status(&project_id).await)
}

#[tauri::command]
pub async fn canon_watch_status(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Option<WatchStatus>> {
    Ok(state.watches.status(&project_id).await)
}
