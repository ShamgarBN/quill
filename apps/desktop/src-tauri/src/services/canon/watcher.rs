//! Filesystem watcher for the project's canon vault (Phase 1.x — wired but
//! not auto-running until Phase 2).
//!
//! Design: spawn a `notify` watcher in a background tokio task; debounce
//! events; emit re-ingest requests through an mpsc channel. The actual
//! re-ingest runs on the main runtime so it shares the Tauri state.

use crate::error::{QuillError, Result};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// A coalesced filesystem change ready for re-ingest.
#[derive(Debug, Clone)]
pub struct VaultChange {
    pub path: PathBuf,
    pub kind: VaultChangeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultChangeKind {
    Created,
    Modified,
    Removed,
}

/// Returns a paired (handle, receiver). Drop the handle to stop watching.
pub struct VaultWatcher {
    _watcher: RecommendedWatcher,
}

pub fn spawn_watcher(
    root: &Path,
    debounce: Duration,
) -> Result<(VaultWatcher, mpsc::Receiver<VaultChange>)> {
    if !root.exists() {
        return Err(QuillError::NotFound(format!(
            "vault root does not exist: {}",
            root.display()
        )));
    }

    let (raw_tx, raw_rx) = mpsc::channel::<notify::Result<Event>>();
    let (out_tx, out_rx) = mpsc::channel::<VaultChange>();

    let mut watcher: RecommendedWatcher =
        RecommendedWatcher::new(raw_tx, Config::default()).map_err(map_notify_err)?;
    watcher
        .watch(root, RecursiveMode::Recursive)
        .map_err(map_notify_err)?;

    // Debouncer thread — coalesces bursts of OS events into one VaultChange
    // per path within the window.
    std::thread::spawn(move || {
        let mut pending: std::collections::HashMap<PathBuf, (VaultChangeKind, Instant)> =
            std::collections::HashMap::new();

        loop {
            // Block until either a new event or the debounce timeout fires.
            let recv = raw_rx.recv_timeout(debounce);
            let now = Instant::now();
            match recv {
                Ok(Ok(ev)) => {
                    let kind = match ev.kind {
                        EventKind::Create(_) => Some(VaultChangeKind::Created),
                        EventKind::Modify(_) => Some(VaultChangeKind::Modified),
                        EventKind::Remove(_) => Some(VaultChangeKind::Removed),
                        _ => None,
                    };
                    if let Some(k) = kind {
                        for p in ev.paths {
                            // Only watch text-ingestible files
                            if is_supported(&p) {
                                pending.insert(p, (k, now));
                            }
                        }
                    }
                }
                Ok(Err(_)) | Err(mpsc::RecvTimeoutError::Timeout) => { /* check pending */ }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }

            // Flush entries that have settled past the debounce window:
            // collect them first, then drop them from the pending map.
            let mut due = Vec::new();
            for (path, (kind, t)) in pending.iter() {
                if now.saturating_duration_since(*t) >= debounce {
                    due.push((path.clone(), *kind));
                }
            }
            pending.retain(|_, (_, t)| now.saturating_duration_since(*t) < debounce);
            for (path, kind) in due {
                let _ = out_tx.send(VaultChange { path, kind });
            }
        }
    });

    Ok((VaultWatcher { _watcher: watcher }, out_rx))
}

fn is_supported(p: &Path) -> bool {
    matches!(
        p.extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase())
            .as_deref(),
        Some("md" | "markdown" | "txt" | "pdf")
    )
}

fn map_notify_err(e: notify::Error) -> QuillError {
    QuillError::Storage(format!("vault watcher: {e}"))
}
