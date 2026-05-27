//! Registry of active vault watchers, keyed by project_id.
//!
//! Owns the `VaultWatcher` (which keeps the OS handle alive), the dispatcher
//! task that consumes `VaultChange` events, and a shared status snapshot the
//! UI can poll via the `canon_watch_status` command.
//!
//! Start a watch with `start()`; it spawns the watcher + dispatcher and
//! returns immediately with the initial status. Stop with `stop()` — drops
//! the watcher, the debouncer thread exits on disconnect, and the dispatcher
//! task observes a closed receiver and exits.

use crate::error::Result;
use crate::models::canon::{ChunkSensitivity, VaultRule};
use crate::services::canon::rules::resolve_sensitivity;
use crate::services::canon::watcher::{spawn_watcher, VaultChange, VaultChangeKind, VaultWatcher};
use crate::services::canon::IngestService;
use crate::services::llm::EmbeddingsProvider;
use crate::services::vector::VectorStore;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::Mutex as AsyncMutex;

/// Default quiescence window. Long enough that "user typing in Obsidian and
/// hitting auto-save every keystroke" coalesces, short enough that a
/// deliberate save shows up quickly.
pub const DEFAULT_DEBOUNCE: Duration = Duration::from_secs(2);

/// Snapshot of a single active watch. Cloned out for IPC return.
#[derive(Debug, Clone, Serialize)]
pub struct WatchStatus {
    pub project_id: String,
    pub vault_path: String,
    pub started_at: DateTime<Utc>,
    pub events_received: u64,
    pub files_reingested: u64,
    pub last_event_at: Option<DateTime<Utc>>,
    pub last_event_path: Option<String>,
    pub last_error: Option<String>,
}

/// The vault rules + default sensitivity in effect for an active watch.
/// Stored behind an RwLock inside `ActiveWatch` so live edits via
/// `update_policy` take effect for the next event without restarting
/// the watcher.
#[derive(Debug, Clone)]
pub struct VaultPolicy {
    pub vault_path: PathBuf,
    pub rules: Vec<VaultRule>,
    pub default: ChunkSensitivity,
}

struct ActiveWatch {
    _watcher: VaultWatcher, // drop = stop watching
    status: Arc<RwLock<WatchStatus>>,
    policy: Arc<RwLock<VaultPolicy>>,
}

pub struct WatchService {
    watches: AsyncMutex<HashMap<String, ActiveWatch>>,
}

impl WatchService {
    pub fn new() -> Self {
        Self {
            watches: AsyncMutex::new(HashMap::new()),
        }
    }

    /// Start watching `vault_path` for `project_id`. If a watch is already
    /// active for that project, it is replaced.
    ///
    /// `embedder` and `vectors` are kept alive by the dispatcher task and
    /// used to re-ingest changed files. `policy` carries the rules and
    /// default sensitivity to apply to each re-ingest; it can be updated
    /// live via `update_policy` (no stop/restart needed).
    pub async fn start(
        &self,
        project_id: &str,
        policy: VaultPolicy,
        embedder: Arc<dyn EmbeddingsProvider>,
        vectors: Arc<dyn VectorStore>,
    ) -> Result<WatchStatus> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<VaultChange>();
        let watcher = spawn_watcher(&policy.vault_path, DEFAULT_DEBOUNCE, tx)?;

        let initial = WatchStatus {
            project_id: project_id.to_string(),
            vault_path: policy.vault_path.to_string_lossy().to_string(),
            started_at: Utc::now(),
            events_received: 0,
            files_reingested: 0,
            last_event_at: None,
            last_event_path: None,
            last_error: None,
        };
        let status = Arc::new(RwLock::new(initial.clone()));
        let policy_lock = Arc::new(RwLock::new(policy));

        // Dispatcher task — owns the receiver, the embedder, the vector
        // store, and clones of status + policy. Exits when rx closes
        // (which happens when the watcher is dropped above).
        let dispatch_status = Arc::clone(&status);
        let dispatch_policy = Arc::clone(&policy_lock);
        let dispatch_project_id = project_id.to_string();
        tokio::spawn(async move {
            while let Some(change) = rx.recv().await {
                // Update event-received counter regardless of ingestion outcome.
                {
                    let mut w = dispatch_status.write().expect("status lock poisoned");
                    w.events_received += 1;
                    w.last_event_at = Some(Utc::now());
                    w.last_event_path = Some(change.path.to_string_lossy().to_string());
                }

                // Snapshot the policy for this event (clone so we don't
                // hold the read lock across an .await).
                let policy_snapshot = dispatch_policy
                    .read()
                    .expect("policy lock poisoned")
                    .clone();
                let result = handle_change(
                    &dispatch_project_id,
                    &change,
                    &policy_snapshot,
                    &*embedder,
                    &*vectors,
                )
                .await;
                let mut w = dispatch_status.write().expect("status lock poisoned");
                match result {
                    Ok(true) => {
                        w.files_reingested += 1;
                        w.last_error = None;
                    }
                    Ok(false) => {
                        // Removal or skipped — no count bump, no error.
                    }
                    Err(e) => {
                        let msg = format!("{}: {}", change.path.display(), e);
                        tracing::warn!(error = %msg, "vault re-ingest failed");
                        w.last_error = Some(msg);
                    }
                }
            }
            tracing::info!(project_id = %dispatch_project_id, "vault dispatcher exiting");
        });

        let mut watches = self.watches.lock().await;
        watches.insert(
            project_id.to_string(),
            ActiveWatch {
                _watcher: watcher,
                status: Arc::clone(&status),
                policy: policy_lock,
            },
        );

        Ok(initial)
    }

    /// Replace the policy in effect for an active watch. No-op if no
    /// watch is active for the project. Used by the rules-editor IPC
    /// path so live edits take effect on the next FS event.
    pub async fn update_policy(&self, project_id: &str, policy: VaultPolicy) -> bool {
        let watches = self.watches.lock().await;
        if let Some(active) = watches.get(project_id) {
            *active.policy.write().expect("policy lock poisoned") = policy;
            true
        } else {
            false
        }
    }

    /// Stop the active watch for `project_id` if any. Returns whether a
    /// watch was actually stopped.
    pub async fn stop(&self, project_id: &str) -> bool {
        let mut watches = self.watches.lock().await;
        watches.remove(project_id).is_some()
    }

    /// Current status for `project_id`, or `None` if no watch is active.
    pub async fn status(&self, project_id: &str) -> Option<WatchStatus> {
        let watches = self.watches.lock().await;
        watches
            .get(project_id)
            .map(|w| w.status.read().expect("status lock poisoned").clone())
    }
}

impl Default for WatchService {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns Ok(true) on successful re-ingest, Ok(false) on a skip (e.g. file
/// no longer exists for a Removed event), Err on ingestion failure.
async fn handle_change(
    project_id: &str,
    change: &VaultChange,
    policy: &VaultPolicy,
    embedder: &dyn EmbeddingsProvider,
    vectors: &dyn VectorStore,
) -> Result<bool> {
    match change.kind {
        VaultChangeKind::Removed => {
            // notify fires Remove for atomic-save patterns (write-temp →
            // rename-over), so we don't act on it. The Modified event for
            // the new file lands separately and triggers re-ingest.
            Ok(false)
        }
        VaultChangeKind::Created | VaultChangeKind::Modified => {
            if !change.path.exists() {
                return Ok(false);
            }
            // Read the file once so we can pass its body to the resolver
            // for frontmatter detection. Failures fall back to no-frontmatter
            // — we'll still resolve via folder rules / default.
            let raw = std::fs::read_to_string(&change.path).ok();
            let sensitivity = resolve_sensitivity(
                &change.path,
                Some(&policy.vault_path),
                &policy.rules,
                policy.default,
                raw.as_deref(),
            );
            let svc = IngestService::new(embedder, vectors);
            svc.ingest_file(project_id, &change.path, None, sensitivity)
                .await?;
            Ok(true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::QuillError;
    use crate::services::llm::MockEmbeddingsProvider;
    use crate::services::vector::JsonVectorStore;

    fn policy(vault_path: PathBuf) -> VaultPolicy {
        VaultPolicy {
            vault_path,
            rules: Vec::new(),
            default: ChunkSensitivity::Public,
        }
    }

    #[tokio::test]
    async fn start_then_stop_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();

        let svc = WatchService::new();
        let embedder: Arc<dyn EmbeddingsProvider> = Arc::new(MockEmbeddingsProvider::new(16));
        let vectors: Arc<dyn VectorStore> =
            Arc::new(JsonVectorStore::open(dir.path().join("v.json")).unwrap());

        let status = svc
            .start("p1", policy(vault), embedder, vectors)
            .await
            .unwrap();
        assert_eq!(status.project_id, "p1");
        assert_eq!(status.events_received, 0);

        assert!(svc.status("p1").await.is_some());
        assert!(svc.stop("p1").await);
        assert!(svc.status("p1").await.is_none());
        // Stopping again is a no-op.
        assert!(!svc.stop("p1").await);
    }

    #[tokio::test]
    async fn start_rejects_missing_vault() {
        let dir = tempfile::tempdir().unwrap();
        let svc = WatchService::new();
        let embedder: Arc<dyn EmbeddingsProvider> = Arc::new(MockEmbeddingsProvider::new(16));
        let vectors: Arc<dyn VectorStore> =
            Arc::new(JsonVectorStore::open(dir.path().join("v.json")).unwrap());
        let err = svc
            .start("p1", policy(dir.path().join("nope")), embedder, vectors)
            .await
            .unwrap_err();
        assert!(matches!(err, QuillError::NotFound(_)));
    }

    #[tokio::test]
    async fn start_replaces_existing_watch_for_same_project() {
        let dir = tempfile::tempdir().unwrap();
        let v1 = dir.path().join("v1");
        let v2 = dir.path().join("v2");
        std::fs::create_dir_all(&v1).unwrap();
        std::fs::create_dir_all(&v2).unwrap();

        let svc = WatchService::new();
        let embedder: Arc<dyn EmbeddingsProvider> = Arc::new(MockEmbeddingsProvider::new(16));
        let vectors: Arc<dyn VectorStore> =
            Arc::new(JsonVectorStore::open(dir.path().join("v.json")).unwrap());

        let _ = svc
            .start(
                "p1",
                policy(v1),
                Arc::clone(&embedder),
                Arc::clone(&vectors),
            )
            .await
            .unwrap();
        let s2 = svc
            .start("p1", policy(v2.clone()), embedder, vectors)
            .await
            .unwrap();
        assert!(s2.vault_path.ends_with("v2"));
        assert!(svc.stop("p1").await);
    }

    #[tokio::test]
    async fn update_policy_changes_active_watch() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let svc = WatchService::new();
        let embedder: Arc<dyn EmbeddingsProvider> = Arc::new(MockEmbeddingsProvider::new(16));
        let vectors: Arc<dyn VectorStore> =
            Arc::new(JsonVectorStore::open(dir.path().join("v.json")).unwrap());
        svc.start("p1", policy(vault.clone()), embedder, vectors)
            .await
            .unwrap();
        // Update with a new default sensitivity.
        let new_policy = VaultPolicy {
            vault_path: vault,
            rules: vec![VaultRule {
                pattern: "DM-Notes".into(),
                sensitivity: ChunkSensitivity::DoNotSend,
            }],
            default: ChunkSensitivity::Spoiler,
        };
        assert!(svc.update_policy("p1", new_policy).await);
        // Stop cleans up.
        assert!(svc.stop("p1").await);
        // Updating with no active watch returns false.
        assert!(
            !svc.update_policy(
                "missing",
                VaultPolicy {
                    vault_path: PathBuf::from("/tmp"),
                    rules: vec![],
                    default: ChunkSensitivity::Public
                }
            )
            .await
        );
    }
}
