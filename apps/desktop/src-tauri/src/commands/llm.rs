//! LLM commands: provider selection, audit log, and a 'ping' that proves
//! credentials work without committing real prose to the model.

use crate::error::{QuillError, Result};
use crate::services::llm::{
    AuditEntry, AuditLog, ChatMessage, ChatRequest, ChatRole, IncludedCategory, ProviderId,
};
use crate::state::AppState;
use chrono::Utc;
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
pub struct ProviderStatus {
    pub provider: ProviderId,
    pub has_key: bool,
}

#[tauri::command]
pub fn llm_provider_status(
    state: State<'_, AppState>,
    provider: ProviderId,
) -> Result<ProviderStatus> {
    let key = provider.secret_key();
    let has_key = if key.is_empty() {
        true
    } else {
        state.secrets.has(key)
    };
    Ok(ProviderStatus { provider, has_key })
}

/// Send a tiny test prompt to verify a provider's credentials work.
/// Logs the call to the audit log so it's visible in the privacy review.
#[tauri::command]
pub async fn llm_ping(state: State<'_, AppState>, provider: ProviderId) -> Result<String> {
    let chat = state.providers.chat(provider)?;
    let req = ChatRequest::new(vec![
        ChatMessage {
            role: ChatRole::System,
            content: "You are a writing-companion ping endpoint. Reply with a single short sentence confirming you are online.".to_string(),
        },
        ChatMessage {
            role: ChatRole::User,
            content: "Ping.".to_string(),
        },
    ]);

    let started = Utc::now();
    let result = chat.chat(&req).await;
    let (success, error) = match &result {
        Ok(_) => (true, None),
        Err(err) => (false, Some(err.to_string())),
    };
    let _ = state.audit.append(&AuditEntry {
        timestamp: started,
        provider: chat.provider_id().to_string(),
        model: chat.model_id().to_string(),
        operation: "ping".to_string(),
        project_id: None,
        scene_id: None,
        tokens_in: result.as_ref().map(|r| r.tokens_in).unwrap_or(0),
        tokens_out: result.as_ref().map(|r| r.tokens_out).unwrap_or(0),
        included: vec![IncludedCategory::SystemPrompt, IncludedCategory::UserPrompt],
        success,
        error,
    });
    let _ = AuditLog::path;
    let r = result?;
    Ok(r.content)
}

#[tauri::command]
pub fn audit_tail(state: State<'_, AppState>, limit: Option<usize>) -> Result<Vec<AuditEntry>> {
    state.audit.tail(limit.unwrap_or(50))
}

/// Open the audit log file in Finder. Used by the Privacy settings link.
#[tauri::command]
pub fn audit_path(state: State<'_, AppState>) -> Result<String> {
    Ok(state.audit.path().to_string_lossy().to_string())
}

#[tauri::command]
pub fn _unused_provider_id_check(_p: ProviderId) -> Result<()> {
    // exists only to ensure ProviderId enum is part of the IPC schema
    Err(QuillError::Internal("unused".into()))
}
