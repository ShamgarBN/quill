//! Filesystem watcher for an Obsidian-style vault directory.
//!
//! Design: spawn a `notify` watcher in a background OS thread; debounce
//! events; emit coalesced `VaultChange` messages into a tokio mpsc channel
//! supplied by the caller. The caller owns the receiver and dispatches the
//! actual re-ingest on the async runtime.
//!
//! Lifetime: the returned `VaultWatcher` keeps the underlying notify handle
//! alive. Drop it to stop watching — the debouncer thread will then exit on
//! its next iteration, the supplied `out_tx` will be dropped, and any
//! receiver awaiting on it will observe `None`.

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

/// Live watcher. Dropping it stops watching and shuts down the debouncer.
pub struct VaultWatcher {
    _watcher: RecommendedWatcher,
}

impl std::fmt::Debug for VaultWatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VaultWatcher").finish_non_exhaustive()
    }
}

/// Spawn a recursive vault watcher rooted at `root`.
///
/// `debounce` is the quiescence window used to coalesce bursts of OS events
/// for the same path. 2 seconds is a sensible default for "user just hit
/// save in Obsidian."
///
/// `out_tx` is the async-friendly sink for coalesced changes. The caller
/// owns the receiver; this function will not poll it.
pub fn spawn_watcher(
    root: &Path,
    debounce: Duration,
    out_tx: tokio::sync::mpsc::UnboundedSender<VaultChange>,
) -> Result<VaultWatcher> {
    if !root.exists() {
        return Err(QuillError::NotFound(format!(
            "vault root does not exist: {}",
            root.display()
        )));
    }
    if !root.is_dir() {
        return Err(QuillError::InvalidArgument(format!(
            "vault path is not a directory: {}",
            root.display()
        )));
    }

    let (raw_tx, raw_rx) = mpsc::channel::<notify::Result<Event>>();

    let mut watcher: RecommendedWatcher =
        RecommendedWatcher::new(raw_tx, Config::default()).map_err(map_notify_err)?;
    watcher
        .watch(root, RecursiveMode::Recursive)
        .map_err(map_notify_err)?;

    // Debouncer thread — coalesces bursts of OS events into one VaultChange
    // per path within the window. Exits when raw_rx returns Disconnected
    // (which happens when the RecommendedWatcher is dropped above us).
    std::thread::spawn(move || {
        let mut pending: std::collections::HashMap<PathBuf, (VaultChangeKind, Instant)> =
            std::collections::HashMap::new();

        loop {
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
                            if is_supported(&p) {
                                pending.insert(p, (k, now));
                            }
                        }
                    }
                }
                Ok(Err(_)) | Err(mpsc::RecvTimeoutError::Timeout) => { /* check pending */ }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }

            // Flush entries that have settled past the debounce window.
            let mut due = Vec::new();
            for (path, (kind, t)) in pending.iter() {
                if now.saturating_duration_since(*t) >= debounce {
                    due.push((path.clone(), *kind));
                }
            }
            pending.retain(|_, (_, t)| now.saturating_duration_since(*t) < debounce);
            for (path, kind) in due {
                if out_tx.send(VaultChange { path, kind }).is_err() {
                    // Receiver dropped; nothing left to do.
                    return;
                }
            }
        }
    });

    Ok(VaultWatcher { _watcher: watcher })
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn rejects_nonexistent_root() {
        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("does-not-exist");
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let err = spawn_watcher(&bad, Duration::from_millis(50), tx).unwrap_err();
        assert!(matches!(err, QuillError::NotFound(_)));
    }

    #[test]
    fn rejects_non_directory_root() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.txt");
        std::fs::write(&f, b"x").unwrap();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let err = spawn_watcher(&f, Duration::from_millis(50), tx).unwrap_err();
        assert!(matches!(err, QuillError::InvalidArgument(_)));
    }

    #[test]
    fn ignores_unsupported_extensions() {
        // Direct unit test of the extension filter, since FS event timing is
        // flaky on macOS CI.
        assert!(is_supported(Path::new("foo.md")));
        assert!(is_supported(Path::new("foo.markdown")));
        assert!(is_supported(Path::new("foo.MD")));
        assert!(is_supported(Path::new("foo.txt")));
        assert!(is_supported(Path::new("foo.pdf")));
        assert!(!is_supported(Path::new("foo.png")));
        assert!(!is_supported(Path::new("foo")));
        assert!(!is_supported(Path::new(".DS_Store")));
    }
}
