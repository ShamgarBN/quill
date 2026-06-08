//! Chat provider trait + request/response shapes.

use crate::error::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    /// 0.0 (deterministic) .. 2.0 (very loose). Default 0.7 for prose.
    pub temperature: f32,
    /// Cap on tokens emitted. Provider implementations clamp to model limits.
    pub max_tokens: u32,
    /// Optional stop sequences.
    pub stop: Vec<String>,
    /// Request strict JSON output. Providers that support a JSON/structured
    /// response mode (e.g. Gemini `responseMimeType`) enable it; others
    /// ignore it. Used by the canon extraction pass.
    #[serde(default)]
    pub json_mode: bool,
    /// Ask the provider to skip extended "thinking" / reasoning tokens.
    /// Gemini 2.5 models think by default, and those tokens count against
    /// the output budget — for bulk extraction that silently truncates
    /// the JSON. Setting this disables thinking so the whole budget goes
    /// to the actual answer. Ignored by providers without a thinking mode.
    #[serde(default)]
    pub disable_thinking: bool,
}

impl ChatRequest {
    pub fn new(messages: Vec<ChatMessage>) -> Self {
        Self {
            messages,
            temperature: 0.7,
            max_tokens: 1024,
            stop: Vec::new(),
            json_mode: false,
            disable_thinking: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub content: String,
    /// Best-effort token counts. Some providers don't return them; we
    /// estimate on the client side so the audit log is still useful.
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub model: String,
}

#[async_trait]
pub trait ChatProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    fn model_id(&self) -> &str;
    async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse>;
}
