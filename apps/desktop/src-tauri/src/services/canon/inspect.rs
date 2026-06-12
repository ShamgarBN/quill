//! Corpus inspector — summarise the contents of the vector index.
//!
//! For each unique `doc_id`, this collapses the chunks into a single
//! `DocSummary` row the UI can render in a table: source path, chunk
//! count, total word count, sensitivity, whether the file still exists
//! on disk, and whether it lives inside the project's vault path.
//!
//! In practice chunks of a doc share the same sensitivity (set at
//! ingest time or by retroactive rule re-apply), but we surface
//! `mixed_sensitivity` defensively in case a future code path
//! mutates chunks of one doc independently.
//!
//! Side-effect-free — purely an aggregation over `chunks_for_project`.

use crate::error::Result;
use crate::models::{CanonKind, ChunkSensitivity};
use crate::services::canon::docs::DocMetaStore;
use crate::services::storage::ProjectStore;
use crate::services::vector::VectorStore;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct DocSummary {
    pub doc_id: String,
    pub source_path: String,
    pub chunk_count: u32,
    pub word_count: u32,
    pub sensitivity: ChunkSensitivity,
    /// True when not every chunk for this doc carries the same sensitivity
    /// tag. Shouldn't happen in normal use but the UI surfaces it so the
    /// user can spot the anomaly.
    pub mixed_sensitivity: bool,
    /// True when the source file still exists on disk.
    pub exists_on_disk: bool,
    /// True when the source path is inside the project's vault directory.
    /// Useful for distinguishing manually-ingested files (often outside
    /// the vault) from watcher-ingested ones.
    pub in_vault: bool,
    /// Per-doc extraction toggle. Defaults true; set false via the
    /// Corpus Inspector to skip this doc's chunks when the auto
    /// extraction pass runs.
    pub extraction_enabled: bool,
    /// Last time the extraction pass completed for this doc, if ever.
    pub last_extracted_at: Option<DateTime<Utc>>,
    /// Canon kind carried by this doc's chunks (first chunk's value;
    /// uniform per doc since ingest replaces wholesale). Preserved on
    /// re-ingest so "re-embed stale" doesn't reset user tagging.
    pub kind: CanonKind,
    /// True when this doc's vectors were produced by a different
    /// embedding model than the one currently configured (or by a
    /// build that pre-dates model tagging). Stale vectors live in an
    /// incompatible space and silently degrade similarity search —
    /// fix by re-ingesting.
    pub embedding_stale: bool,
}

pub async fn list_documents(
    vectors: &dyn VectorStore,
    project_id: &str,
    vault_path: Option<&Path>,
    projects: Option<&ProjectStore>,
    current_embed_model: Option<&str>,
) -> Result<Vec<DocSummary>> {
    let chunks = vectors.chunks_for_project(project_id).await?;
    // Group by doc_id, preserving stable ordering via BTreeMap.
    let mut groups: BTreeMap<String, GroupAcc> = BTreeMap::new();
    for c in chunks {
        let acc = groups.entry(c.doc_id.clone()).or_insert_with(|| GroupAcc {
            source_path: c.source_path.clone(),
            chunk_count: 0,
            word_count: 0,
            first_sensitivity: c.sensitivity,
            mixed: false,
            kind: c.kind,
            embedding_model: c.embedding_model.clone(),
        });
        acc.chunk_count += 1;
        acc.word_count += c.word_count;
        if c.sensitivity != acc.first_sensitivity {
            acc.mixed = true;
        }
    }

    // Join in per-doc metadata (extraction toggle, last-extracted timestamp).
    // Bulk-load once instead of fetching per doc.
    let metas: Vec<crate::models::canon::DocMeta> = match projects {
        Some(p) => DocMetaStore::new(p).list(project_id)?,
        None => Vec::new(),
    };
    let meta_for = |doc_id: &str| -> (bool, Option<DateTime<Utc>>) {
        metas
            .iter()
            .find(|m| m.doc_id == doc_id)
            .map(|m| (m.extraction_enabled, m.last_extracted_at))
            .unwrap_or((true, None))
    };

    let mut out: Vec<DocSummary> = groups
        .into_iter()
        .map(|(doc_id, g)| {
            let exists_on_disk = !g.source_path.is_empty() && Path::new(&g.source_path).exists();
            let in_vault = match vault_path {
                Some(vp) if !g.source_path.is_empty() => Path::new(&g.source_path).starts_with(vp),
                _ => false,
            };
            let (extraction_enabled, last_extracted_at) = meta_for(&doc_id);
            // Stale = we know the current model and this doc's vectors
            // came from something else (or pre-date tagging entirely).
            let embedding_stale = current_embed_model
                .map(|m| g.embedding_model != m)
                .unwrap_or(false);
            DocSummary {
                doc_id,
                source_path: g.source_path,
                chunk_count: g.chunk_count,
                word_count: g.word_count,
                sensitivity: g.first_sensitivity,
                mixed_sensitivity: g.mixed,
                exists_on_disk,
                in_vault,
                extraction_enabled,
                last_extracted_at,
                kind: g.kind,
                embedding_stale,
            }
        })
        .collect();
    // Sort by source_path for predictable display order.
    out.sort_by(|a, b| a.source_path.cmp(&b.source_path));
    Ok(out)
}

/// Walk every doc in the project's index, check whether its source path
/// still exists on disk, and delete chunks for missing files. Returns
/// the count of docs pruned.
///
/// Files without a recorded source_path (v0.2 chunks pre-dating that
/// field) are left alone — we can't tell whether they're missing.
pub async fn prune_missing(vectors: &dyn VectorStore, project_id: &str) -> Result<u64> {
    let docs = list_documents(vectors, project_id, None, None, None).await?;
    let mut pruned = 0u64;
    for doc in docs {
        if doc.source_path.is_empty() {
            continue;
        }
        if !doc.exists_on_disk {
            vectors.delete_by_doc(&doc.doc_id).await?;
            pruned += 1;
        }
    }
    Ok(pruned)
}

/// Bulk-retag every chunk belonging to the given doc ids. Returns the
/// count of chunks that actually changed.
pub async fn retag_documents(
    vectors: &dyn VectorStore,
    project_id: &str,
    doc_ids: &[String],
    new_sensitivity: ChunkSensitivity,
) -> Result<u64> {
    if doc_ids.is_empty() {
        return Ok(0);
    }
    let wanted: std::collections::HashSet<&str> = doc_ids.iter().map(String::as_str).collect();
    let chunks = vectors.chunks_for_project(project_id).await?;
    let updates: Vec<(String, ChunkSensitivity)> = chunks
        .into_iter()
        .filter(|c| wanted.contains(c.doc_id.as_str()))
        .map(|c| (c.id, new_sensitivity))
        .collect();
    vectors.update_sensitivities(&updates).await
}

struct GroupAcc {
    source_path: String,
    chunk_count: u32,
    word_count: u32,
    first_sensitivity: ChunkSensitivity,
    mixed: bool,
    kind: CanonKind,
    embedding_model: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CanonChunk;
    use crate::services::vector::JsonVectorStore;
    use std::path::PathBuf;

    fn chunk(
        id: &str,
        doc: &str,
        project: &str,
        source: &str,
        sensitivity: ChunkSensitivity,
        words: u32,
    ) -> CanonChunk {
        CanonChunk {
            id: id.into(),
            doc_id: doc.into(),
            project_id: project.into(),
            index: 0,
            offset: 0,
            text: "x".into(),
            headings: vec![],
            word_count: words,
            sensitivity,
            source_path: source.into(),
            kind: crate::models::CanonKind::Lore,
            embedding_model: String::new(),
        }
    }

    #[tokio::test]
    async fn embedding_staleness_detected_against_current_model() {
        let dir = tempfile::tempdir().unwrap();
        let vectors = JsonVectorStore::open(dir.path().join("v.json")).unwrap();
        let mut fresh = chunk("a:0", "doc_a", "p", "/a.md", ChunkSensitivity::Public, 10);
        fresh.embedding_model = "gemini-embedding-001".into();
        // doc_b pre-dates model tagging (empty model string).
        let legacy = chunk("b:0", "doc_b", "p", "/b.md", ChunkSensitivity::Public, 10);
        vectors
            .insert_many(&[(fresh, vec![1.0]), (legacy, vec![1.0])])
            .await
            .unwrap();

        let docs = list_documents(&vectors, "p", None, None, Some("gemini-embedding-001"))
            .await
            .unwrap();
        let a = docs.iter().find(|d| d.doc_id == "doc_a").unwrap();
        let b = docs.iter().find(|d| d.doc_id == "doc_b").unwrap();
        assert!(!a.embedding_stale, "matching model is fresh");
        assert!(b.embedding_stale, "untagged legacy chunks are stale");

        // Without a current model (no API key), staleness is unknown → false.
        let docs = list_documents(&vectors, "p", None, None, None)
            .await
            .unwrap();
        assert!(docs.iter().all(|d| !d.embedding_stale));
    }

    #[tokio::test]
    async fn list_documents_groups_chunks_by_doc() {
        let dir = tempfile::tempdir().unwrap();
        let vectors = JsonVectorStore::open(dir.path().join("v.json")).unwrap();
        vectors
            .insert_many(&[
                (
                    chunk(
                        "a:0",
                        "doc_a",
                        "p",
                        "/vault/Characters/kaelan.md",
                        ChunkSensitivity::Public,
                        100,
                    ),
                    vec![1.0, 0.0],
                ),
                (
                    chunk(
                        "a:1",
                        "doc_a",
                        "p",
                        "/vault/Characters/kaelan.md",
                        ChunkSensitivity::Public,
                        80,
                    ),
                    vec![1.0, 0.0],
                ),
                (
                    chunk(
                        "b:0",
                        "doc_b",
                        "p",
                        "/vault/DM-Notes/secret.md",
                        ChunkSensitivity::DoNotSend,
                        50,
                    ),
                    vec![0.0, 1.0],
                ),
            ])
            .await
            .unwrap();
        let vault = PathBuf::from("/vault");
        let docs = list_documents(&vectors, "p", Some(&vault), None, None)
            .await
            .unwrap();
        assert_eq!(docs.len(), 2);
        let a = docs.iter().find(|d| d.doc_id == "doc_a").unwrap();
        assert_eq!(a.chunk_count, 2);
        assert_eq!(a.word_count, 180);
        assert!(a.in_vault);
        assert!(!a.mixed_sensitivity);
        let b = docs.iter().find(|d| d.doc_id == "doc_b").unwrap();
        assert_eq!(b.chunk_count, 1);
        assert_eq!(b.sensitivity, ChunkSensitivity::DoNotSend);
    }

    #[tokio::test]
    async fn mixed_sensitivity_flag_when_chunks_diverge() {
        let dir = tempfile::tempdir().unwrap();
        let vectors = JsonVectorStore::open(dir.path().join("v.json")).unwrap();
        vectors
            .insert_many(&[
                (
                    chunk("a:0", "doc_a", "p", "/x.md", ChunkSensitivity::Public, 10),
                    vec![1.0],
                ),
                (
                    chunk("a:1", "doc_a", "p", "/x.md", ChunkSensitivity::Spoiler, 10),
                    vec![1.0],
                ),
            ])
            .await
            .unwrap();
        let docs = list_documents(&vectors, "p", None, None, None)
            .await
            .unwrap();
        assert!(docs[0].mixed_sensitivity);
    }

    #[tokio::test]
    async fn prune_missing_removes_docs_for_vanished_files() {
        let dir = tempfile::tempdir().unwrap();
        let vectors = JsonVectorStore::open(dir.path().join("v.json")).unwrap();
        let real_path = dir.path().join("real.md");
        std::fs::write(&real_path, "exists").unwrap();
        vectors
            .insert_many(&[
                (
                    chunk(
                        "a:0",
                        "doc_a",
                        "p",
                        real_path.to_str().unwrap(),
                        ChunkSensitivity::Public,
                        10,
                    ),
                    vec![1.0],
                ),
                (
                    chunk(
                        "b:0",
                        "doc_b",
                        "p",
                        "/tmp/definitely-not-real-and-i-hope-not.md",
                        ChunkSensitivity::Public,
                        10,
                    ),
                    vec![1.0],
                ),
            ])
            .await
            .unwrap();
        let pruned = prune_missing(&vectors, "p").await.unwrap();
        assert_eq!(pruned, 1);
        let remaining = list_documents(&vectors, "p", None, None, None)
            .await
            .unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].doc_id, "doc_a");
    }

    #[tokio::test]
    async fn retag_documents_updates_only_requested_docs() {
        let dir = tempfile::tempdir().unwrap();
        let vectors = JsonVectorStore::open(dir.path().join("v.json")).unwrap();
        vectors
            .insert_many(&[
                (
                    chunk("a:0", "doc_a", "p", "/a.md", ChunkSensitivity::Public, 10),
                    vec![1.0],
                ),
                (
                    chunk("a:1", "doc_a", "p", "/a.md", ChunkSensitivity::Public, 10),
                    vec![1.0],
                ),
                (
                    chunk("b:0", "doc_b", "p", "/b.md", ChunkSensitivity::Public, 10),
                    vec![1.0],
                ),
            ])
            .await
            .unwrap();
        let changed = retag_documents(
            &vectors,
            "p",
            &["doc_a".into()],
            ChunkSensitivity::DoNotSend,
        )
        .await
        .unwrap();
        assert_eq!(changed, 2);
        let docs = list_documents(&vectors, "p", None, None, None)
            .await
            .unwrap();
        let a = docs.iter().find(|d| d.doc_id == "doc_a").unwrap();
        let b = docs.iter().find(|d| d.doc_id == "doc_b").unwrap();
        assert_eq!(a.sensitivity, ChunkSensitivity::DoNotSend);
        assert_eq!(b.sensitivity, ChunkSensitivity::Public);
    }
}
