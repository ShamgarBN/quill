//! Embeddings provider trait.

use crate::error::Result;
use async_trait::async_trait;

#[async_trait]
pub trait EmbeddingsProvider: Send + Sync {
    /// Dimensionality of the produced vectors.
    fn dimensions(&self) -> usize;

    /// Provider identifier (e.g. "gemini-text-embedding-004", "mock-32d").
    fn model_id(&self) -> &str;

    /// Embed a batch of texts. Implementations MUST preserve input order.
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
}
