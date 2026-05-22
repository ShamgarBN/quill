//! Canon ingestion + retrieval commands.

use crate::error::{QuillError, Result};
use crate::models::{CanonKind, ChunkRef, ChunkSensitivity};
use crate::services::canon::{IngestReport, IngestService};
use crate::services::llm::ProviderId;
use crate::state::AppState;
use std::path::PathBuf;
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
