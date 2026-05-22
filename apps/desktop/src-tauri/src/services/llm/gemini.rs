//! Google Gemini (AI Studio) provider implementation.
//!
//! Endpoints (May 2026):
//! - chat:       POST https://generativelanguage.googleapis.com/v1beta/models/<model>:generateContent
//! - embeddings: POST https://generativelanguage.googleapis.com/v1beta/models/<model>:batchEmbedContents
//!
//! API key is sent via `?key=...` query parameter (Gemini convention).
//!
//! IMPORTANT: Free-tier inputs are used to train Google models. See
//! docs/PRIVACY.md. We do NOT touch this concern at the provider layer —
//! the privacy disclosure flow lives in the UI and gates access to chat
//! commands at the command-handler boundary.

use crate::error::{QuillError, Result};
use crate::services::llm::{ChatProvider, ChatRequest, ChatResponse, ChatRole, EmbeddingsProvider};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub const DEFAULT_CHAT_MODEL: &str = "gemini-2.5-pro";
pub const DEFAULT_EMBED_MODEL: &str = "text-embedding-004";
pub const EMBED_DIMS: usize = 768;
const BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";

// ---------- Chat ----------

pub struct GeminiChat {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl GeminiChat {
    pub fn new(api_key: String) -> Self {
        Self::with_model(api_key, DEFAULT_CHAT_MODEL.to_string())
    }

    pub fn with_model(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model,
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Serialize)]
struct GeminiContent<'a> {
    role: &'a str,
    parts: Vec<GeminiPart<'a>>,
}

#[derive(Serialize)]
struct GeminiPart<'a> {
    text: &'a str,
}

#[derive(Serialize)]
struct GenerationConfig {
    temperature: f32,
    #[serde(rename = "maxOutputTokens")]
    max_output_tokens: u32,
    #[serde(rename = "stopSequences", skip_serializing_if = "Vec::is_empty")]
    stop_sequences: Vec<String>,
}

#[derive(Serialize)]
struct GenerateRequest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "systemInstruction")]
    system_instruction: Option<GeminiContent<'a>>,
    contents: Vec<GeminiContent<'a>>,
    #[serde(rename = "generationConfig")]
    generation_config: GenerationConfig,
}

#[derive(Deserialize)]
struct GenerateResponse {
    candidates: Option<Vec<Candidate>>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<UsageMetadata>,
    #[serde(rename = "promptFeedback")]
    prompt_feedback: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct Candidate {
    content: Option<CandidateContent>,
}

#[derive(Deserialize)]
struct CandidateContent {
    parts: Option<Vec<CandidatePart>>,
}

#[derive(Deserialize)]
struct CandidatePart {
    text: Option<String>,
}

#[derive(Deserialize)]
struct UsageMetadata {
    #[serde(rename = "promptTokenCount", default)]
    prompt_token_count: u32,
    #[serde(rename = "candidatesTokenCount", default)]
    candidates_token_count: u32,
}

#[async_trait]
impl ChatProvider for GeminiChat {
    fn provider_id(&self) -> &str {
        "gemini"
    }

    fn model_id(&self) -> &str {
        &self.model
    }

    async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse> {
        // Gemini puts the system prompt in a separate field; the rest is a
        // user/model alternation.
        let mut system_text = String::new();
        let mut contents = Vec::new();
        for m in &req.messages {
            match m.role {
                ChatRole::System => {
                    if !system_text.is_empty() {
                        system_text.push_str("\n\n");
                    }
                    system_text.push_str(&m.content);
                }
                ChatRole::User => contents.push(GeminiContent {
                    role: "user",
                    parts: vec![GeminiPart { text: &m.content }],
                }),
                ChatRole::Assistant => contents.push(GeminiContent {
                    role: "model",
                    parts: vec![GeminiPart { text: &m.content }],
                }),
            }
        }

        let body = GenerateRequest {
            system_instruction: if system_text.is_empty() {
                None
            } else {
                Some(GeminiContent {
                    role: "system",
                    parts: vec![GeminiPart { text: &system_text }],
                })
            },
            contents,
            generation_config: GenerationConfig {
                temperature: req.temperature,
                max_output_tokens: req.max_tokens.min(8192),
                stop_sequences: req.stop.clone(),
            },
        };

        let url = format!(
            "{BASE_URL}/models/{model}:generateContent?key={key}",
            model = urlenc(&self.model),
            key = urlenc(&self.api_key),
        );
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| QuillError::Storage(format!("gemini chat request: {e}")))?;
        if !resp.status().is_success() {
            let code = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(QuillError::Storage(format!(
                "gemini chat HTTP {code}: {}",
                redact_key(&body, &self.api_key)
            )));
        }
        let parsed: GenerateResponse = resp
            .json()
            .await
            .map_err(|e| QuillError::Storage(format!("gemini chat parse: {e}")))?;

        let content = parsed
            .candidates
            .and_then(|cs| cs.into_iter().next())
            .and_then(|c| c.content)
            .and_then(|c| c.parts)
            .and_then(|ps| ps.into_iter().filter_map(|p| p.text).next())
            .ok_or_else(|| {
                QuillError::Storage(format!(
                    "gemini returned no candidates (prompt feedback: {:?})",
                    parsed.prompt_feedback
                ))
            })?;

        let usage = parsed.usage_metadata.unwrap_or(UsageMetadata {
            prompt_token_count: 0,
            candidates_token_count: 0,
        });

        Ok(ChatResponse {
            content,
            tokens_in: usage.prompt_token_count,
            tokens_out: usage.candidates_token_count,
            model: self.model.clone(),
        })
    }
}

// ---------- Embeddings ----------

pub struct GeminiEmbeddings {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl GeminiEmbeddings {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            model: DEFAULT_EMBED_MODEL.to_string(),
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Serialize)]
struct BatchEmbedRequest<'a> {
    requests: Vec<EmbedSubrequest<'a>>,
}

#[derive(Serialize)]
struct EmbedSubrequest<'a> {
    model: String,
    content: GeminiContent<'a>,
    #[serde(rename = "taskType")]
    task_type: &'a str,
}

#[derive(Deserialize)]
struct BatchEmbedResponse {
    embeddings: Vec<EmbeddingValues>,
}

#[derive(Deserialize)]
struct EmbeddingValues {
    values: Vec<f32>,
}

#[async_trait]
impl EmbeddingsProvider for GeminiEmbeddings {
    fn dimensions(&self) -> usize {
        EMBED_DIMS
    }

    fn model_id(&self) -> &str {
        &self.model
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let model_path = format!("models/{}", self.model);
        let body = BatchEmbedRequest {
            requests: texts
                .iter()
                .map(|t| EmbedSubrequest {
                    model: model_path.clone(),
                    content: GeminiContent {
                        role: "user",
                        parts: vec![GeminiPart { text: t }],
                    },
                    task_type: "RETRIEVAL_DOCUMENT",
                })
                .collect(),
        };

        let url = format!(
            "{BASE_URL}/{model_path}:batchEmbedContents?key={key}",
            key = urlenc(&self.api_key),
        );
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| QuillError::Storage(format!("gemini embed request: {e}")))?;
        if !resp.status().is_success() {
            let code = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(QuillError::Storage(format!(
                "gemini embed HTTP {code}: {}",
                redact_key(&body, &self.api_key)
            )));
        }
        let parsed: BatchEmbedResponse = resp
            .json()
            .await
            .map_err(|e| QuillError::Storage(format!("gemini embed parse: {e}")))?;
        Ok(parsed.embeddings.into_iter().map(|e| e.values).collect())
    }
}

// ---------- helpers ----------

/// Minimal URL-component percent encoder for API keys and model names.
/// Avoids pulling in `urlencoding` for one call site.
fn urlenc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            other => {
                let _ = std::fmt::Write::write_fmt(&mut out, format_args!("%{other:02X}"));
            }
        }
    }
    out
}

/// Defensive: never echo an API key in an error message, even if Gemini
/// happened to reflect it back.
fn redact_key(s: &str, key: &str) -> String {
    if key.is_empty() {
        return s.to_string();
    }
    s.replace(key, "[REDACTED]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urlenc_roundtrip_safe_chars() {
        assert_eq!(urlenc("abcXYZ09-_.~"), "abcXYZ09-_.~");
        assert_eq!(urlenc(" "), "%20");
        assert_eq!(urlenc("a/b?c=1"), "a%2Fb%3Fc%3D1");
    }

    #[test]
    fn redact_key_replaces_occurrences() {
        let key = "AIzaSy_TESTKEY";
        let body = format!("error with key {key} repeated {key}");
        let out = redact_key(&body, key);
        assert!(!out.contains(key));
        assert!(out.contains("[REDACTED]"));
    }
}
