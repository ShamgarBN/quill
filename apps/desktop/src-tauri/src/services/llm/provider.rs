//! Pluggable provider registry.
//!
//! At runtime, the user picks one chat provider and one embeddings provider
//! via Settings. The registry resolves a `ProviderId` to a concrete trait
//! object, given the user's API keys (looked up via the SecretStore).

use crate::error::{QuillError, Result};
use crate::services::crypto::SecretStore;
use crate::services::llm::{ChatProvider, EmbeddingsProvider};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderId {
    /// Google Gemini via AI Studio API.
    Gemini,
    /// Groq's hosted Llama 3.3 70B (drafting fallback).
    Groq,
    /// Mock provider — used for tests; never selectable in production UI.
    Mock,
}

impl ProviderId {
    pub fn secret_key(self) -> &'static str {
        match self {
            ProviderId::Gemini => "QUILL_GEMINI_API_KEY",
            ProviderId::Groq => "QUILL_GROQ_API_KEY",
            ProviderId::Mock => "",
        }
    }
}

pub struct ProviderRegistry {
    pub secrets: Arc<SecretStore>,
}

impl ProviderRegistry {
    pub fn new(secrets: Arc<SecretStore>) -> Self {
        Self { secrets }
    }

    pub fn chat(&self, id: ProviderId) -> Result<Arc<dyn ChatProvider>> {
        match id {
            ProviderId::Gemini => {
                let key = self.require_key(id)?;
                Ok(Arc::new(super::gemini::GeminiChat::new(key)))
            }
            ProviderId::Groq => {
                let key = self.require_key(id)?;
                Ok(Arc::new(super::groq::GroqChat::new(key)))
            }
            ProviderId::Mock => Ok(Arc::new(super::mock::MockChatProvider::echo())),
        }
    }

    pub fn embeddings(&self, id: ProviderId) -> Result<Arc<dyn EmbeddingsProvider>> {
        match id {
            ProviderId::Gemini => {
                let key = self.require_key(id)?;
                Ok(Arc::new(super::gemini::GeminiEmbeddings::new(key)))
            }
            ProviderId::Groq => Err(QuillError::InvalidArgument(
                "Groq does not provide an embeddings endpoint".into(),
            )),
            ProviderId::Mock => Ok(Arc::new(super::mock::MockEmbeddingsProvider::new(64))),
        }
    }

    fn require_key(&self, id: ProviderId) -> Result<String> {
        let k = id.secret_key();
        let v = self.secrets.get(k)?.ok_or_else(|| {
            QuillError::InvalidArgument(format!(
                "missing API key for {:?}: set it in Settings → Privacy first",
                id
            ))
        })?;
        if v.trim().is_empty() {
            return Err(QuillError::InvalidArgument(format!(
                "API key for {:?} is empty",
                id
            )));
        }
        Ok(v)
    }
}
