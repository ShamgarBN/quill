//! Groq provider implementation (chat only — Groq does not provide
//! embeddings).
//!
//! Groq exposes an OpenAI-compatible chat-completions endpoint. We use
//! Llama 3.3 70B as the default fallback model; it's free-tier-friendly
//! and produces decent on-genre prose.

use crate::error::{QuillError, Result};
use crate::services::llm::{ChatMessage, ChatProvider, ChatRequest, ChatResponse, ChatRole};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub const DEFAULT_CHAT_MODEL: &str = "llama-3.3-70b-versatile";
const BASE_URL: &str = "https://api.groq.com/openai/v1";

pub struct GroqChat {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl GroqChat {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            model: DEFAULT_CHAT_MODEL.to_string(),
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Serialize)]
struct OaiMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct OaiRequest<'a> {
    model: &'a str,
    messages: Vec<OaiMessage<'a>>,
    temperature: f32,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stop: Vec<String>,
}

#[derive(Deserialize)]
struct OaiResponse {
    choices: Vec<OaiChoice>,
    usage: Option<OaiUsage>,
    model: Option<String>,
}

#[derive(Deserialize)]
struct OaiChoice {
    message: OaiMessageOwned,
}

#[derive(Deserialize)]
struct OaiMessageOwned {
    content: String,
}

#[derive(Deserialize, Default)]
struct OaiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
}

fn role_str(r: ChatRole) -> &'static str {
    match r {
        ChatRole::System => "system",
        ChatRole::User => "user",
        ChatRole::Assistant => "assistant",
    }
}

#[async_trait]
impl ChatProvider for GroqChat {
    fn provider_id(&self) -> &str {
        "groq"
    }

    fn model_id(&self) -> &str {
        &self.model
    }

    async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse> {
        let messages: Vec<OaiMessage> = req
            .messages
            .iter()
            .map(|m: &ChatMessage| OaiMessage {
                role: role_str(m.role),
                content: &m.content,
            })
            .collect();
        let body = OaiRequest {
            model: &self.model,
            messages,
            temperature: req.temperature,
            max_tokens: req.max_tokens.min(8192),
            stop: req.stop.clone(),
        };

        let resp = self
            .client
            .post(format!("{BASE_URL}/chat/completions"))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| QuillError::Storage(format!("groq request: {e}")))?;
        if !resp.status().is_success() {
            let code = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(QuillError::Storage(format!("groq HTTP {code}: {body}")));
        }
        let parsed: OaiResponse = resp
            .json()
            .await
            .map_err(|e| QuillError::Storage(format!("groq parse: {e}")))?;
        let content = parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| QuillError::Storage("groq returned no choices".into()))?;
        let usage = parsed.usage.unwrap_or_default();
        Ok(ChatResponse {
            content,
            tokens_in: usage.prompt_tokens,
            tokens_out: usage.completion_tokens,
            model: parsed.model.unwrap_or_else(|| self.model.clone()),
        })
    }
}
