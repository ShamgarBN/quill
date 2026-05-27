//! Ingest orchestrator: ties extract → chunk → embed → store together.

use crate::error::{QuillError, Result};
use crate::models::canon::{CanonChunk, CanonDocument, ChunkRef, ChunkSensitivity};
use crate::services::canon::chunker::{chunk_markdown, chunk_plain, ChunkOptions};
use crate::services::canon::extract::{extract_from_path, Extracted, SourceKind};
use crate::services::llm::EmbeddingsProvider;
use crate::services::vector::VectorStore;
use chrono::Utc;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct IngestReport {
    pub document: CanonDocument,
    pub chunks_emitted: u32,
    pub bytes_read: u64,
}

pub struct IngestService<'a> {
    pub embedder: &'a dyn EmbeddingsProvider,
    pub vectors: &'a dyn VectorStore,
}

impl<'a> IngestService<'a> {
    pub fn new(embedder: &'a dyn EmbeddingsProvider, vectors: &'a dyn VectorStore) -> Self {
        Self { embedder, vectors }
    }

    /// Ingest a single file, replacing any prior version of it (matched by
    /// canonicalized path).
    pub async fn ingest_file(
        &self,
        project_id: &str,
        path: &Path,
        kind_override: Option<crate::models::canon::CanonKind>,
        sensitivity: ChunkSensitivity,
    ) -> Result<IngestReport> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let bytes_read = std::fs::metadata(&canonical).map(|m| m.len()).unwrap_or(0);
        let Extracted { kind, text } = extract_from_path(&canonical)?;

        let opts = ChunkOptions::default();
        let chunks = match kind {
            SourceKind::Markdown => chunk_markdown(&text, opts),
            SourceKind::Plain | SourceKind::Pdf => chunk_plain(&text, opts),
        };

        if chunks.is_empty() {
            return Err(QuillError::InvalidArgument(
                "document produced no chunks (file empty?)".into(),
            ));
        }

        let texts: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();
        let embeddings = self.embedder.embed_batch(&texts).await?;
        if embeddings.len() != chunks.len() {
            return Err(QuillError::Internal(format!(
                "embedding count mismatch: {} embeddings for {} chunks",
                embeddings.len(),
                chunks.len()
            )));
        }

        let doc_id = compute_doc_id(&canonical);
        let source_path = canonical.to_string_lossy().to_string();
        let now = Utc::now();
        let chunks_emitted = chunks.len() as u32;
        let document = CanonDocument {
            id: doc_id.clone(),
            project_id: project_id.to_string(),
            source_path: source_path.clone(),
            kind: kind_override.unwrap_or(crate::models::canon::CanonKind::Lore),
            source_kind: kind,
            ingested_at: now,
            updated_at: now,
            chunk_count: chunks_emitted,
            byte_size: bytes_read,
        };

        // Replace any prior chunks for this doc, then insert fresh ones.
        self.vectors.delete_by_doc(&doc_id).await?;
        let mut canon_chunks = Vec::with_capacity(chunks.len());
        for (chunk, embedding) in chunks.into_iter().zip(embeddings) {
            let id = format!("{doc_id}:{}", chunk.index);
            canon_chunks.push((
                CanonChunk {
                    id: id.clone(),
                    doc_id: doc_id.clone(),
                    project_id: project_id.to_string(),
                    index: chunk.index,
                    offset: chunk.offset,
                    text: chunk.text,
                    headings: chunk.headings,
                    word_count: chunk.word_count,
                    sensitivity,
                    source_path: source_path.clone(),
                },
                embedding,
            ));
        }
        self.vectors.insert_many(&canon_chunks).await?;

        Ok(IngestReport {
            document,
            chunks_emitted,
            bytes_read,
        })
    }

    /// Retrieve the top-K chunks most relevant to `query`, optionally
    /// excluding chunks marked do-not-send.
    pub async fn retrieve(
        &self,
        project_id: &str,
        query: &str,
        k: usize,
        respect_do_not_send: bool,
    ) -> Result<Vec<ChunkRef>> {
        let q_embeds = self.embedder.embed_batch(&[query]).await?;
        let q_vec = q_embeds
            .into_iter()
            .next()
            .ok_or_else(|| QuillError::Internal("embedder returned no vector".into()))?;
        self.vectors
            .search(project_id, &q_vec, k, respect_do_not_send)
            .await
    }
}

fn compute_doc_id(canonical: &Path) -> String {
    let mut h = Sha256::new();
    h.update(canonical.to_string_lossy().as_bytes());
    let digest = h.finalize();
    let hex: String = digest.iter().take(12).map(|b| format!("{b:02x}")).collect();
    format!("doc_{hex}")
}

#[allow(dead_code)]
pub fn doc_id_for(path: &Path) -> String {
    let canonical = path.canonicalize().unwrap_or_else(|_| PathBuf::from(path));
    compute_doc_id(&canonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::llm::MockEmbeddingsProvider;
    use crate::services::vector::JsonVectorStore;

    #[tokio::test]
    async fn ingest_markdown_then_search() {
        let dir = tempfile::tempdir().unwrap();
        let canon_dir = dir.path().join("canon");
        std::fs::create_dir_all(&canon_dir).unwrap();
        let f = canon_dir.join("locations.md");
        std::fs::write(
            &f,
            "# The Hollow Wastes\n\nA scorched expanse where dragons once nested.\n\n## Lake Tarn\n\nA cold mirror at the western edge.\n",
        )
        .unwrap();

        let embedder = MockEmbeddingsProvider::new(64);
        let vectors = JsonVectorStore::open(dir.path().join("vectors.json")).unwrap();

        let svc = IngestService::new(&embedder, &vectors);
        let report = svc
            .ingest_file("p1", &f, None, ChunkSensitivity::Public)
            .await
            .unwrap();
        assert_eq!(report.chunks_emitted, 2);
        assert_eq!(report.document.project_id, "p1");

        let hits = svc.retrieve("p1", "lake", 5, true).await.unwrap();
        assert!(!hits.is_empty());
        // The "Lake Tarn" chunk should appear and carry its breadcrumb.
        assert!(hits
            .iter()
            .any(|h| h.headings.iter().any(|s| s == "Lake Tarn")));
    }

    #[tokio::test]
    async fn re_ingest_replaces_prior_chunks() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.md");
        std::fs::write(&f, "# v1\n\nfirst body\n").unwrap();
        let embedder = MockEmbeddingsProvider::new(32);
        let vectors = JsonVectorStore::open(dir.path().join("v.json")).unwrap();
        let svc = IngestService::new(&embedder, &vectors);

        svc.ingest_file("p", &f, None, ChunkSensitivity::Public)
            .await
            .unwrap();
        let count1 = vectors.count_for_project("p").await.unwrap();
        assert_eq!(count1, 1);

        std::fs::write(
            &f,
            "# v2\n\nfirst body\n\n## new\n\nsecond body that's longer to force a chunk\n",
        )
        .unwrap();
        svc.ingest_file("p", &f, None, ChunkSensitivity::Public)
            .await
            .unwrap();
        let count2 = vectors.count_for_project("p").await.unwrap();
        assert_eq!(count2, 2, "should have replaced, not duplicated");
    }
}
