//! LLM provider layer.
//!
//! Two distinct concerns live here:
//! - Embeddings: turning text into vectors for retrieval.
//! - Chat: generating prose / critique.
//!
//! Both are abstracted behind traits so we can mock for tests, swap providers
//! at runtime via settings, and (Phase 9) drop in local models without
//! touching call-sites.

mod audit;
mod chat;
mod embeddings;
mod mock;
mod provider;

// Real provider implementations
pub mod gemini;
pub mod groq;

pub use audit::{AuditEntry, AuditLog, IncludedCategory};
pub use chat::{ChatMessage, ChatProvider, ChatRequest, ChatResponse, ChatRole};
pub use embeddings::EmbeddingsProvider;
#[allow(unused_imports)]
pub use mock::{MockChatProvider, MockEmbeddingsProvider};
pub use provider::{ProviderId, ProviderRegistry};
