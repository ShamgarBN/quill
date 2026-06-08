//! Canon ingestion + retrieval commands.

use crate::error::{QuillError, Result};
use crate::models::canon::DocMeta;
use crate::models::{CanonKind, ChunkRef, ChunkSensitivity};
use crate::services::canon::{
    extract_and_merge, list_documents, prune_missing, reapply_rules, resolve_sensitivity,
    retag_documents, DocMetaStore, DocSummary, ExtractionReport, IngestReport, IngestService,
    VaultPolicy, WatchStatus,
};
use crate::services::llm::{AuditEntry, AuditLog, IncludedCategory, ProviderId};
use crate::services::storage::ProjectStore;
use crate::services::vector::VectorStore;
use chrono::Utc;
use std::sync::Arc;
use crate::state::AppState;
use serde_json::json;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, State};

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
    app: AppHandle,
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
    let report = svc
        .ingest_file(&project_id, &path_buf, kind, resolved_sensitivity)
        .await?;

    // Background entity extraction. Fire-and-forget so the IPC call
    // returns promptly; the UI gets refreshed via the
    // `canon-extraction-complete` event when the LLM round-trip lands.
    let outcome = schedule_extraction(
        &app,
        &state,
        &project_id,
        &report.document.id,
        ExtractionTrigger::Auto,
    );
    if let ScheduleOutcome::SkippedNoProvider(reason) = outcome {
        emit_skip(&app, &report.document.id, reason);
    }

    Ok(report)
}

/// Why an extraction run was scheduled — drives whether we skip when
/// extraction is toggled off (Auto) or force regardless (Manual).
#[derive(Debug, Clone, Copy)]
enum ExtractionTrigger {
    Auto,
    Manual,
}

/// Result of scheduling an extraction run.
#[derive(Debug, Clone, Copy)]
enum ScheduleOutcome {
    /// Background task spawned successfully — completion will land via
    /// `canon-extraction-complete`.
    Spawned,
    /// Skipped because the user opted this doc out of extraction.
    SkippedDisabled,
    /// Skipped because the chat provider is Mock or unconfigured. The
    /// caller (or auto path) emits a `complete` event so the UI can
    /// surface a friendly error.
    SkippedNoProvider(&'static str),
}

/// Spawn the background extraction task. Clones owned handles so the
/// task can outlive this command. Returns a status so callers can
/// surface "no provider" errors synchronously instead of leaving the
/// UI spinning indefinitely.
fn schedule_extraction(
    app: &AppHandle,
    state: &AppState,
    project_id: &str,
    doc_id: &str,
    trigger: ExtractionTrigger,
) -> ScheduleOutcome {
    // Quick gate: respect per-doc extraction toggle for Auto runs.
    if matches!(trigger, ExtractionTrigger::Auto) {
        let meta = DocMetaStore::new(&state.projects)
            .get(project_id, doc_id)
            .unwrap_or_else(|_| DocMeta::defaults_for(doc_id));
        if !meta.extraction_enabled {
            return ScheduleOutcome::SkippedDisabled;
        }
    }

    // Resolve the chat provider from settings. If it's missing or Mock,
    // we have nothing to call — extraction relies on real-LLM JSON output.
    let settings = match state.settings_store.load_or_init() {
        Ok(s) => s,
        Err(_) => {
            return ScheduleOutcome::SkippedNoProvider(
                "Could not load settings to resolve chat provider.",
            );
        }
    };
    let chat_id = settings.chat_provider;
    if matches!(chat_id, ProviderId::Mock) {
        return ScheduleOutcome::SkippedNoProvider(
            "No chat provider configured — set one in Settings → Privacy to enable AI extraction.",
        );
    }
    let chat = match state.providers.chat_for_extraction(chat_id) {
        Ok(c) => c,
        Err(_) => {
            return ScheduleOutcome::SkippedNoProvider(
                "Chat provider API key missing — add it in Settings → Privacy.",
            );
        }
    };

    let projects = state.projects.clone();
    let vectors = state.vectors.clone();
    let audit = state.audit.clone();
    let project_id_owned = project_id.to_string();
    let doc_id_owned = doc_id.to_string();
    let app_owned = app.clone();

    // Announce immediately so the UI can show a spinner / progress chip.
    let _ = app.emit(
        "canon-extraction-started",
        json!({ "project_id": project_id_owned, "doc_id": doc_id_owned }),
    );

    tokio::spawn(async move {
        let provider_name = chat.provider_id().to_string();
        let model_name = chat.model_id().to_string();
        let outcome = run_extraction(
            &projects,
            vectors.as_ref(),
            chat.as_ref(),
            &project_id_owned,
            &doc_id_owned,
        )
        .await;
        // Always write an audit entry so the user can see what happened
        // in Settings → Audit log — even when extraction failed or found
        // nothing. That's the diagnostic backstop.
        write_extraction_audit(&audit, &provider_name, &model_name, &project_id_owned, &outcome);
        let payload = match outcome {
            Ok(report) => json!({
                "doc_id": doc_id_owned,
                "report": report,
                "error": serde_json::Value::Null,
            }),
            Err(e) => json!({
                "doc_id": doc_id_owned,
                "report": ExtractionReport::default(),
                "error": e.to_string(),
            }),
        };
        let _ = app_owned.emit("canon-extraction-complete", payload);
    });
    ScheduleOutcome::Spawned
}

/// Append a single audit row for an extraction attempt. Best-effort —
/// audit failures are logged but never bubble up to the user.
fn write_extraction_audit(
    audit: &Arc<AuditLog>,
    provider: &str,
    model: &str,
    project_id: &str,
    outcome: &Result<ExtractionReport>,
) {
    let (success, error, tokens_out) = match outcome {
        Ok(r) => {
            let total = r.characters_added + r.ideas_added + r.threads_added;
            (true, None, total)
        }
        Err(e) => (false, Some(e.to_string()), 0),
    };
    let entry = AuditEntry {
        timestamp: Utc::now(),
        provider: provider.to_string(),
        model: model.to_string(),
        operation: "canon_extraction".to_string(),
        project_id: Some(project_id.to_string()),
        scene_id: None,
        tokens_in: 0,
        tokens_out,
        included: vec![IncludedCategory::CanonTopK],
        success,
        error,
    };
    if let Err(e) = audit.append(&entry) {
        tracing::warn!(error = %e, "canon extraction audit append failed");
    }
}

/// Emit a synthetic `canon-extraction-complete` so the UI can clear
/// any pending state and show a friendly error. Used when the run was
/// skipped synchronously and no background task will fire one.
fn emit_skip(app: &AppHandle, doc_id: &str, reason: &str) {
    let _ = app.emit(
        "canon-extraction-complete",
        json!({
            "doc_id": doc_id,
            "report": ExtractionReport::default(),
            "error": reason,
        }),
    );
}

async fn run_extraction(
    projects: &ProjectStore,
    vectors: &dyn VectorStore,
    chat: &dyn crate::services::llm::ChatProvider,
    project_id: &str,
    doc_id: &str,
) -> Result<ExtractionReport> {
    let chunks = vectors
        .chunks_for_project(project_id)
        .await?
        .into_iter()
        .filter(|c| c.doc_id == doc_id)
        .collect::<Vec<_>>();
    extract_and_merge(project_id, doc_id, &chunks, chat, projects).await
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
    list_documents(&*state.vectors, &project_id, vault_path, Some(&state.projects)).await
}

/// Remove every chunk belonging to a document. Returns the count of
/// chunks that were removed (mostly diagnostic). Also drops the doc's
/// metadata entry so toggle state doesn't leak across re-ingests of a
/// freshly-different file at the same path.
#[tauri::command]
pub async fn canon_delete_document(
    state: State<'_, AppState>,
    project_id: String,
    doc_id: String,
) -> Result<u64> {
    let removed = state.vectors.delete_by_doc(&doc_id).await?;
    let _ = DocMetaStore::new(&state.projects).forget(&project_id, &doc_id);
    Ok(removed)
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

// ---------- Entity extraction controls ----------

/// Toggle whether the auto-extraction pass runs for this doc. When false,
/// re-ingesting / vault-watcher updates / manual triggers all skip it.
#[tauri::command]
pub async fn canon_set_doc_extraction(
    state: State<'_, AppState>,
    project_id: String,
    doc_id: String,
    enabled: bool,
) -> Result<DocMeta> {
    DocMetaStore::new(&state.projects).set_extraction_enabled(&project_id, &doc_id, enabled)
}

/// Manually run extraction for a single doc. Useful for re-extracting
/// after editing a doc, or after enabling extraction for a doc that
/// was previously opted out. Returns the report so the UI can display
/// counts; also emits `canon-extraction-complete` for parity with the
/// auto path.
#[tauri::command]
pub async fn canon_extract_doc(
    state: State<'_, AppState>,
    app: AppHandle,
    project_id: String,
    doc_id: String,
) -> Result<()> {
    match schedule_extraction(&app, &state, &project_id, &doc_id, ExtractionTrigger::Manual) {
        ScheduleOutcome::Spawned => Ok(()),
        ScheduleOutcome::SkippedDisabled => Err(QuillError::InvalidArgument(
            "AI extraction is disabled for this document; enable it first.".into(),
        )),
        ScheduleOutcome::SkippedNoProvider(reason) => {
            Err(QuillError::InvalidArgument(reason.to_string()))
        }
    }
}
