//! Per-key trailing-edge debouncer for background work.
//!
//! Used to coalesce vault-watcher re-ingests into one extraction pass:
//! Obsidian autosaves every few seconds while the user types, and each
//! save re-ingests the file. Without a quiet period, every save would
//! burn an LLM call. `schedule` resets the timer for its key, so the
//! work fires once, `delay` after the *last* save.
//!
//! Semantics: trailing-edge only, per-key. Re-scheduling an in-flight
//! key aborts the pending (still-sleeping) task. Once a task has woken
//! and started running it can no longer be aborted — a concurrent
//! re-schedule may then produce one extra run, which is acceptable for
//! idempotent work like extraction (enrichment dedupes by name).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task::JoinHandle;

pub struct Debouncer {
    delay: Duration,
    pending: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
}

impl Debouncer {
    pub fn new(delay: Duration) -> Self {
        Self {
            delay,
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Schedule `run` to fire after the quiet period. Calling again with
    /// the same key before it fires resets the timer.
    ///
    /// Must be called from within a tokio runtime.
    pub fn schedule(&self, key: &str, run: impl FnOnce() + Send + 'static) {
        let mut pending = self.pending.lock().expect("debounce lock poisoned");
        if let Some(prev) = pending.remove(key) {
            prev.abort();
        }
        let delay = self.delay;
        let map = Arc::clone(&self.pending);
        let key_owned = key.to_string();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            // Deregister before running so a re-schedule issued *during*
            // `run` starts a fresh timer instead of aborting nothing.
            map.lock()
                .expect("debounce lock poisoned")
                .remove(&key_owned);
            run();
        });
        pending.insert(key.to_string(), handle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    async fn settle(counter: &AtomicU32, expect: u32) {
        // Jump the paused clock past the debounce deadline in one step
        // (auto-advance is instant), then yield so woken tasks run.
        tokio::time::sleep(Duration::from_secs(120)).await;
        for _ in 0..50 {
            if counter.load(Ordering::SeqCst) == expect {
                return;
            }
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn coalesces_rapid_schedules_into_one_run() {
        let d = Debouncer::new(Duration::from_secs(90));
        let counter = Arc::new(AtomicU32::new(0));
        for _ in 0..5 {
            let c = Arc::clone(&counter);
            d.schedule("doc_a", move || {
                c.fetch_add(1, Ordering::SeqCst);
            });
        }
        settle(&counter, 1).await;
        assert_eq!(counter.load(Ordering::SeqCst), 1, "five saves → one run");
    }

    #[tokio::test(start_paused = true)]
    async fn distinct_keys_run_independently() {
        let d = Debouncer::new(Duration::from_secs(90));
        let counter = Arc::new(AtomicU32::new(0));
        for key in ["doc_a", "doc_b", "doc_c"] {
            let c = Arc::clone(&counter);
            d.schedule(key, move || {
                c.fetch_add(1, Ordering::SeqCst);
            });
        }
        settle(&counter, 3).await;
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn key_can_fire_again_after_completion() {
        let d = Debouncer::new(Duration::from_secs(90));
        let counter = Arc::new(AtomicU32::new(0));
        let c = Arc::clone(&counter);
        d.schedule("doc_a", move || {
            c.fetch_add(1, Ordering::SeqCst);
        });
        settle(&counter, 1).await;
        let c = Arc::clone(&counter);
        d.schedule("doc_a", move || {
            c.fetch_add(1, Ordering::SeqCst);
        });
        settle(&counter, 2).await;
        assert_eq!(
            counter.load(Ordering::SeqCst),
            2,
            "same key fires again later"
        );
    }
}
