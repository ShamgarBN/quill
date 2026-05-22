//! Drafting commands — Phase 6 entry points from the manuscript editor.

use crate::error::Result;
use crate::services::drafting::{DraftPreview, DraftRequest, DraftSuggestion, DraftingService};
use crate::state::AppState;
use tauri::State;

/// Build the prompt that *would* be sent and return a preview. No LLM call.
#[tauri::command]
pub async fn drafting_preview(
    state: State<'_, AppState>,
    req: DraftRequest,
) -> Result<DraftPreview> {
    let settings = state.settings_store.load_or_init()?;
    let chat = state.providers.chat(settings.chat_provider)?;
    let embedder = state.providers.embeddings(settings.embedding_provider)?;
    let svc = DraftingService {
        projects: &state.projects,
        vectors: state.vectors.clone(),
        embedder,
        providers: &state.providers,
        audit: state.audit.clone(),
    };
    svc.preview(chat, &req).await
}

/// Run the full drafting pipeline (drift gate + LLM call + audit). Returns
/// the suggestion content along with metadata so the UI can render the
/// side-by-side panel.
#[tauri::command]
pub async fn drafting_invoke(
    state: State<'_, AppState>,
    req: DraftRequest,
) -> Result<DraftSuggestion> {
    let settings = state.settings_store.load_or_init()?;
    let chat = state.providers.chat(settings.chat_provider)?;
    let embedder = state.providers.embeddings(settings.embedding_provider)?;
    let svc = DraftingService {
        projects: &state.projects,
        vectors: state.vectors.clone(),
        embedder,
        providers: &state.providers,
        audit: state.audit.clone(),
    };
    svc.invoke(chat, &req).await
}
