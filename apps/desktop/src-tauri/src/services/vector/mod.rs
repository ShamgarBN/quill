//! Vector store abstraction.
//!
//! Phase 1 ships with `JsonVectorStore`: an in-memory cosine-similarity
//! search backed by a JSON file. At hobby scale (≤2k chunks of 768-dim
//! vectors ≈ 6 MB), this is faster than spinning up a real DB and
//! eliminates a deployment surface.
//!
//! Phase 9 (optional) swaps in LanceDB without changing call-sites — they
//! all go through the `VectorStore` trait.

mod json_store;

pub use json_store::JsonVectorStore;

use crate::error::Result;
use crate::models::{CanonChunk, ChunkRef, ChunkSensitivity};
use async_trait::async_trait;

#[async_trait]
pub trait VectorStore: Send + Sync {
    /// Insert (or upsert) a batch of chunk + embedding pairs.
    async fn insert_many(&self, items: &[(CanonChunk, Vec<f32>)]) -> Result<()>;

    /// Remove all chunks belonging to a document.
    async fn delete_by_doc(&self, doc_id: &str) -> Result<u64>;

    /// Top-K cosine-similarity search scoped to a project.
    async fn search(
        &self,
        project_id: &str,
        query: &[f32],
        k: usize,
        respect_do_not_send: bool,
    ) -> Result<Vec<ChunkRef>>;

    /// Diagnostic: how many chunks for a project.
    async fn count_for_project(&self, project_id: &str) -> Result<u64>;

    /// Walk every chunk for a project. Used by cross-link queries that
    /// need exact text matching rather than similarity search.
    async fn chunks_for_project(&self, project_id: &str) -> Result<Vec<CanonChunk>>;

    /// Update the sensitivity tag on a set of chunks by id. Used by the
    /// retroactive vault-rules re-apply path. Returns the count of chunks
    /// actually modified (i.e. whose new tag differed from the old one).
    async fn update_sensitivities(&self, updates: &[(String, ChunkSensitivity)]) -> Result<u64>;
}
