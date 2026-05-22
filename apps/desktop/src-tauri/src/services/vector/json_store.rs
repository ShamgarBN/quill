//! JSON-backed in-memory vector store.
//!
//! On open, loads everything into a Vec. Mutations are atomic via the
//! storage::atomic helpers. Search is brute-force cosine — at our hobby
//! scale (a few thousand chunks max) that's microseconds; we don't need
//! ANN indexing yet.

use crate::error::{QuillError, Result};
use crate::models::{CanonChunk, ChunkRef, ChunkSensitivity};
use crate::services::storage;
use crate::services::vector::VectorStore;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredEntry {
    chunk: CanonChunk,
    embedding: Vec<f32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct StoredFile {
    version: u8,
    entries: Vec<StoredEntry>,
}

pub struct JsonVectorStore {
    path: PathBuf,
    inner: RwLock<StoredFile>,
}

impl JsonVectorStore {
    pub fn open(path: PathBuf) -> Result<Self> {
        let inner = if path.exists() {
            storage::atomic_read_json_or_default::<StoredFile>(&path)?
        } else {
            StoredFile {
                version: 1,
                entries: Vec::new(),
            }
        };
        Ok(Self {
            path,
            inner: RwLock::new(inner),
        })
    }

    fn flush(&self, file: &StoredFile) -> Result<()> {
        storage::atomic_write_json(&self.path, file)
    }
}

#[async_trait]
impl VectorStore for JsonVectorStore {
    async fn insert_many(&self, items: &[(CanonChunk, Vec<f32>)]) -> Result<()> {
        if items.is_empty() {
            return Ok(());
        }
        let mut g = self
            .inner
            .write()
            .map_err(|_| QuillError::Internal("vector store lock poisoned".into()))?;
        for (chunk, emb) in items {
            // Replace if id exists, else append.
            if let Some(pos) = g.entries.iter().position(|e| e.chunk.id == chunk.id) {
                g.entries[pos] = StoredEntry {
                    chunk: chunk.clone(),
                    embedding: emb.clone(),
                };
            } else {
                g.entries.push(StoredEntry {
                    chunk: chunk.clone(),
                    embedding: emb.clone(),
                });
            }
        }
        self.flush(&g)
    }

    async fn delete_by_doc(&self, doc_id: &str) -> Result<u64> {
        let mut g = self
            .inner
            .write()
            .map_err(|_| QuillError::Internal("vector store lock poisoned".into()))?;
        let before = g.entries.len();
        g.entries.retain(|e| e.chunk.doc_id != doc_id);
        let removed = (before - g.entries.len()) as u64;
        self.flush(&g)?;
        Ok(removed)
    }

    async fn search(
        &self,
        project_id: &str,
        query: &[f32],
        k: usize,
        respect_do_not_send: bool,
    ) -> Result<Vec<ChunkRef>> {
        let g = self
            .inner
            .read()
            .map_err(|_| QuillError::Internal("vector store lock poisoned".into()))?;
        let q_norm = norm(query);
        if q_norm == 0.0 {
            return Ok(Vec::new());
        }
        let mut scored: Vec<(f32, &CanonChunk)> = g
            .entries
            .iter()
            .filter(|e| e.chunk.project_id == project_id)
            .filter(|e| {
                if respect_do_not_send {
                    !matches!(e.chunk.sensitivity, ChunkSensitivity::DoNotSend)
                } else {
                    true
                }
            })
            .map(|e| {
                let s = cosine(query, &e.embedding, q_norm);
                (s, &e.chunk)
            })
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(k);
        Ok(scored
            .into_iter()
            .map(|(s, c)| ChunkRef::from_chunk(c, s))
            .collect())
    }

    async fn count_for_project(&self, project_id: &str) -> Result<u64> {
        let g = self
            .inner
            .read()
            .map_err(|_| QuillError::Internal("vector store lock poisoned".into()))?;
        Ok(g.entries
            .iter()
            .filter(|e| e.chunk.project_id == project_id)
            .count() as u64)
    }

    async fn chunks_for_project(&self, project_id: &str) -> Result<Vec<CanonChunk>> {
        let g = self
            .inner
            .read()
            .map_err(|_| QuillError::Internal("vector store lock poisoned".into()))?;
        Ok(g.entries
            .iter()
            .filter(|e| e.chunk.project_id == project_id)
            .map(|e| e.chunk.clone())
            .collect())
    }
}

fn norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

fn cosine(q: &[f32], v: &[f32], q_norm: f32) -> f32 {
    if q.len() != v.len() {
        return 0.0;
    }
    let dot: f32 = q.iter().zip(v.iter()).map(|(a, b)| a * b).sum();
    let v_norm = norm(v);
    if v_norm == 0.0 {
        0.0
    } else {
        dot / (q_norm * v_norm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ChunkSensitivity;

    fn chunk(id: &str, doc: &str, project: &str, text: &str) -> CanonChunk {
        CanonChunk {
            id: id.to_string(),
            doc_id: doc.to_string(),
            project_id: project.to_string(),
            index: 0,
            offset: 0,
            text: text.to_string(),
            headings: Vec::new(),
            word_count: text.split_whitespace().count() as u32,
            sensitivity: ChunkSensitivity::Public,
        }
    }

    #[tokio::test]
    async fn insert_and_search() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonVectorStore::open(dir.path().join("v.json")).unwrap();
        let items = vec![
            (chunk("a:0", "a", "p", "alpha"), vec![1.0, 0.0, 0.0]),
            (chunk("b:0", "b", "p", "beta"), vec![0.0, 1.0, 0.0]),
            (chunk("c:0", "c", "p", "gamma"), vec![0.7, 0.7, 0.0]),
        ];
        store.insert_many(&items).await.unwrap();
        let hits = store.search("p", &[1.0, 0.0, 0.0], 2, true).await.unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].id, "a:0");
        assert_eq!(hits[1].id, "c:0");
        assert!(hits[0].score > hits[1].score);
    }

    #[tokio::test]
    async fn project_isolation() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonVectorStore::open(dir.path().join("v.json")).unwrap();
        store
            .insert_many(&[
                (chunk("p1:0", "d1", "p1", "x"), vec![1.0, 0.0]),
                (chunk("p2:0", "d2", "p2", "x"), vec![1.0, 0.0]),
            ])
            .await
            .unwrap();
        assert_eq!(store.count_for_project("p1").await.unwrap(), 1);
        let hits = store.search("p1", &[1.0, 0.0], 5, true).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].project_id, "p1");
    }

    #[tokio::test]
    async fn do_not_send_is_respected() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonVectorStore::open(dir.path().join("v.json")).unwrap();
        let mut secret = chunk("s:0", "d", "p", "secret reveal");
        secret.sensitivity = ChunkSensitivity::DoNotSend;
        store
            .insert_many(&[
                (chunk("a:0", "d", "p", "ok"), vec![1.0, 0.0]),
                (secret, vec![1.0, 0.0]),
            ])
            .await
            .unwrap();
        let with_filter = store.search("p", &[1.0, 0.0], 5, true).await.unwrap();
        assert_eq!(with_filter.len(), 1);
        assert_eq!(with_filter[0].id, "a:0");
        let without_filter = store.search("p", &[1.0, 0.0], 5, false).await.unwrap();
        assert_eq!(without_filter.len(), 2);
    }

    #[tokio::test]
    async fn delete_by_doc_removes_only_that_doc() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonVectorStore::open(dir.path().join("v.json")).unwrap();
        store
            .insert_many(&[
                (chunk("d1:0", "d1", "p", "a"), vec![1.0, 0.0]),
                (chunk("d1:1", "d1", "p", "b"), vec![0.0, 1.0]),
                (chunk("d2:0", "d2", "p", "c"), vec![0.5, 0.5]),
            ])
            .await
            .unwrap();
        let removed = store.delete_by_doc("d1").await.unwrap();
        assert_eq!(removed, 2);
        assert_eq!(store.count_for_project("p").await.unwrap(), 1);
    }

    #[tokio::test]
    async fn persists_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v.json");
        {
            let store = JsonVectorStore::open(path.clone()).unwrap();
            store
                .insert_many(&[(chunk("a:0", "d", "p", "x"), vec![1.0, 0.0])])
                .await
                .unwrap();
        }
        let store = JsonVectorStore::open(path).unwrap();
        assert_eq!(store.count_for_project("p").await.unwrap(), 1);
    }
}
