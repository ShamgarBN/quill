//! Canon ingestion service.
//!
//! Ingests worldbuilding source material — PDFs and Markdown files — into
//! a chunked, embedded, retrievable form.
//!
//! Pipeline:
//!   raw file → text extraction → semantic chunking → embedding → vector store
//!
//! Each subsystem is a separate module so individual stages can be tested
//! in isolation:
//! - `extract`  — turn raw bytes into plain text + structural hints
//! - `chunker`  — split text into ~400–800 token chunks with overlap
//! - `ingest`   — orchestrates the pipeline end-to-end
//! - `watcher`  — Obsidian vault filesystem watcher (Phase 1.x)

pub mod chunker;
pub mod extract;
pub mod ingest;
pub mod rules;
pub mod watch_service;
pub mod watcher;

#[allow(unused_imports)]
pub use chunker::{chunk_markdown, chunk_plain, Chunk, ChunkOptions};
pub use ingest::{IngestReport, IngestService};
pub use rules::{reapply_rules, resolve_sensitivity};
pub use watch_service::{VaultPolicy, WatchService, WatchStatus};
