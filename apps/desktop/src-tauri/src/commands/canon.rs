//! Canon ingestion + retrieval commands.

use crate::error::{QuillError, Result};
use crate::models::{CanonKind, ChunkRef, ChunkSensitivity};
use crate::services::canon::{
    list_documents, prune_missing, reapply_rules, resolve_sensitivity, retag_documents, DocSummary,
    IngestReport, IngestService, VaultPolicy, WatchStatus,
};
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
    // Resolve sensitivity: explicit > frontmatter > folder rules > project default.
    let resolved_sensitivity = match sensitivity {
        Some(s) => s,
        None => {
            let project = state.projects.open(&project_id)?;
            let vault_path = project.vault_path.as_deref().map(Path::new);
            let raw = std::fs::read_to_string(&path_buf).ok();
            resolve_sensitivity(
                &path_buf,
                vault_path,
                &project.vault_rules,
                project.vault_default_sensitivity,
                raw.as_deref(),
            )
        }
    };
    let svc = IngestService::new(&*embedder, &*state.vectors);
    svc.ingest_file(&project_id, &path_buf, kind, resolved_sensitivity)
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

    let policy = VaultPolicy {
        vault_path: path,
        rules: project.vault_rules.clone(),
        default: project.vault_default_sensitivity,
    };
    let status = state
        .watches
        .start(&project_id, policy, embedder, vectors)
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

/// Apply the project's current vault rules + default sensitivity
/// retroactively to every existing chunk in the index. Returns the count
/// of chunks whose sensitivity actually changed.
///
/// Also propagates the new policy to the active watcher (if any) so
/// future re-ingests pick up the same rules.
#[tauri::command]
pub async fn canon_reapply_rules(state: State<'_, AppState>, project_id: String) -> Result<u64> {
    let project = state.projects.open(&project_id)?;
    let vault_path = project.vault_path.as_deref().map(Path::new);
    // Push the new policy to the live watcher if there is one.
    if let Some(vp) = vault_path {
        let policy = VaultPolicy {
            vault_path: vp.to_path_buf(),
            rules: project.vault_rules.clone(),
            default: project.vault_default_sensitivity,
        };
        state.watches.update_policy(&project_id, policy).await;
    }
    reapply_rules(
        &*state.vectors,
        &project_id,
        vault_path,
        &project.vault_rules,
        project.vault_default_sensitivity,
    )
    .await
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

// ---------- Corpus inspector ----------

/// List every document in the project's canon index, with per-doc
/// summary metadata. Used by the corpus inspector UI in the Canon view.
#[tauri::command]
pub async fn canon_list_documents(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<DocSummary>> {
    let project = state.projects.open(&project_id)?;
    let vault_path = project.vault_path.as_deref().map(Path::new);
    list_documents(&*state.vectors, &project_id, vault_path).await
}

/// Remove every chunk belonging to a document. Returns the count of
/// chunks that were removed (mostly diagnostic).
#[tauri::command]
pub async fn canon_delete_document(
    state: State<'_, AppState>,
    _project_id: String,
    doc_id: String,
) -> Result<u64> {
    state.vectors.delete_by_doc(&doc_id).await
}

/// Walk every doc in the project; for any whose source file no longer
/// exists on disk, delete its chunks. Returns the count of docs pruned.
#[tauri::command]
pub async fn canon_prune_missing(state: State<'_, AppState>, project_id: String) -> Result<u64> {
    prune_missing(&*state.vectors, &project_id).await
}

/// Bulk-retag every chunk belonging to the given doc ids to a single
/// sensitivity. Lets the user override rules for specific docs.
#[tauri::command]
pub async fn canon_retag_documents(
    state: State<'_, AppState>,
    project_id: String,
    doc_ids: Vec<String>,
    sensitivity: ChunkSensitivity,
) -> Result<u64> {
    retag_documents(&*state.vectors, &project_id, &doc_ids, sensitivity).await
}
