//! Daily word-count snapshots — the "+ N words today" counter.
//!
//! Strategy: persist a small JSON file keyed by date with the manuscript
//! word count at the start of that day. The first call on any given day
//! creates the snapshot from the current total; subsequent calls return
//! `current - snapshot` as today's delta.
//!
//! Edge case: a writing session that crosses midnight will reset the
//! counter and the post-midnight words will count toward the new day's
//! total. This is the desired behavior — "today" means today.
//!
//! Storage: `<project>/structure/word_count_snapshots.json`.
//! Only the last ~60 days are retained; older entries are pruned on every
//! save to keep the file small.

use crate::error::Result;
use crate::services::storage::{self, ProjectStore};
use chrono::{DateTime, Local, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

const SNAPSHOT_FILE: &str = "word_count_snapshots.json";
const MAX_RETAINED_DAYS: usize = 60;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct SnapshotFile {
    /// Date (YYYY-MM-DD, local time) → manuscript word count at start of day.
    snapshots: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TodayProgress {
    pub date: String,
    pub current_total: u64,
    pub baseline: u64,
    pub delta: i64,
    /// Yesterday's delta if available, for the "yesterday: +N" sub-line.
    pub previous_delta: Option<i64>,
}

pub struct ProgressService<'a> {
    pub projects: &'a ProjectStore,
}

impl<'a> ProgressService<'a> {
    pub fn new(projects: &'a ProjectStore) -> Self {
        Self { projects }
    }

    fn snapshot_path(&self, project_id: &str) -> Result<PathBuf> {
        let root = self.projects.root_dir(project_id)?;
        let dir = root.join("structure");
        std::fs::create_dir_all(&dir)?;
        Ok(dir.join(SNAPSHOT_FILE))
    }

    fn load(&self, project_id: &str) -> Result<SnapshotFile> {
        let path = self.snapshot_path(project_id)?;
        if !path.exists() {
            return Ok(SnapshotFile::default());
        }
        let bytes = std::fs::read(&path)?;
        Ok(serde_json::from_slice(&bytes).unwrap_or_default())
    }

    fn save(&self, project_id: &str, file: &SnapshotFile) -> Result<()> {
        let path = self.snapshot_path(project_id)?;
        storage::atomic_write_json(&path, file)
    }

    /// Compute today's progress. Creates a baseline for today if one doesn't
    /// exist yet (using `current_total`). Prunes snapshots older than
    /// `MAX_RETAINED_DAYS`.
    pub fn today(&self, project_id: &str, current_total: u64) -> Result<TodayProgress> {
        let now: DateTime<Local> = DateTime::<Utc>::from(std::time::SystemTime::now()).into();
        let today = now.date_naive();
        self.compute_for_date(project_id, current_total, today)
    }

    /// Same as `today` but lets the caller supply the date — used in tests.
    pub fn compute_for_date(
        &self,
        project_id: &str,
        current_total: u64,
        today: NaiveDate,
    ) -> Result<TodayProgress> {
        let mut file = self.load(project_id)?;
        let key = today.format("%Y-%m-%d").to_string();

        let baseline = match file.snapshots.get(&key).copied() {
            Some(b) => b,
            None => {
                // First reading today — snapshot the current total as baseline.
                file.snapshots.insert(key.clone(), current_total);
                current_total
            }
        };

        // Compute previous-day delta if we have at least two distinct dates.
        let previous_delta = previous_day_delta(&file, &key);

        prune(&mut file);
        self.save(project_id, &file)?;

        Ok(TodayProgress {
            date: key,
            current_total,
            baseline,
            delta: current_total as i64 - baseline as i64,
            previous_delta,
        })
    }
}

fn previous_day_delta(file: &SnapshotFile, today_key: &str) -> Option<i64> {
    // We don't store an end-of-day total, only a start-of-day baseline. So
    // "yesterday's writing" is approximated as today's baseline minus the
    // most recent prior day's baseline. (If they didn't open the app
    // yesterday this conflates several days, which is acceptable.)
    let today_baseline = *file.snapshots.get(today_key)?;
    let (_, prev_baseline) = file.snapshots.range(..today_key.to_string()).next_back()?;
    Some(today_baseline as i64 - *prev_baseline as i64)
}

fn prune(file: &mut SnapshotFile) {
    if file.snapshots.len() <= MAX_RETAINED_DAYS {
        return;
    }
    let excess = file.snapshots.len() - MAX_RETAINED_DAYS;
    let keys_to_remove: Vec<String> = file.snapshots.keys().take(excess).cloned().collect();
    for k in keys_to_remove {
        file.snapshots.remove(&k);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, ProjectStore, String) {
        let dir = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(dir.path());
        let p = projects.create("Demo").unwrap();
        (dir, projects, p.id)
    }

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn first_call_today_baselines_at_current_total() {
        let (_d, ps, pid) = fixture();
        let svc = ProgressService::new(&ps);
        let p = svc.compute_for_date(&pid, 1000, d("2026-05-26")).unwrap();
        assert_eq!(p.baseline, 1000);
        assert_eq!(p.current_total, 1000);
        assert_eq!(p.delta, 0);
    }

    #[test]
    fn subsequent_call_same_day_returns_delta() {
        let (_d, ps, pid) = fixture();
        let svc = ProgressService::new(&ps);
        let _ = svc.compute_for_date(&pid, 1000, d("2026-05-26")).unwrap();
        let p = svc.compute_for_date(&pid, 1350, d("2026-05-26")).unwrap();
        assert_eq!(p.baseline, 1000);
        assert_eq!(p.delta, 350);
    }

    #[test]
    fn new_day_creates_new_baseline_at_current_total() {
        let (_d, ps, pid) = fixture();
        let svc = ProgressService::new(&ps);
        // Day 1: write 1000 words.
        svc.compute_for_date(&pid, 1000, d("2026-05-26")).unwrap();
        svc.compute_for_date(&pid, 1500, d("2026-05-26")).unwrap();
        // Day 2: counter resets.
        let p = svc.compute_for_date(&pid, 1500, d("2026-05-27")).unwrap();
        assert_eq!(p.baseline, 1500);
        assert_eq!(p.delta, 0);
        // Add a few more.
        let p = svc.compute_for_date(&pid, 1700, d("2026-05-27")).unwrap();
        assert_eq!(p.delta, 200);
    }

    #[test]
    fn previous_delta_is_yesterday_to_today_baseline_diff() {
        let (_d, ps, pid) = fixture();
        let svc = ProgressService::new(&ps);
        // Day 1 baseline 1000, ended at 1500
        svc.compute_for_date(&pid, 1000, d("2026-05-26")).unwrap();
        svc.compute_for_date(&pid, 1500, d("2026-05-26")).unwrap();
        // Day 2 baseline 1500
        let p = svc.compute_for_date(&pid, 1500, d("2026-05-27")).unwrap();
        // previous_delta = today's baseline - yesterday's baseline = 1500-1000 = 500.
        assert_eq!(p.previous_delta, Some(500));
    }

    #[test]
    fn prune_keeps_only_recent_days() {
        let (_d, ps, pid) = fixture();
        let svc = ProgressService::new(&ps);
        // Seed many days using date arithmetic so we don't hit month boundaries.
        let start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let last_day = start + chrono::Duration::days(79);
        for offset in 0..80 {
            let date = start + chrono::Duration::days(offset);
            svc.compute_for_date(&pid, offset as u64 * 100, date)
                .unwrap();
        }
        let file = svc.load(&pid).unwrap();
        assert!(file.snapshots.len() <= MAX_RETAINED_DAYS);
        // The most recent date should still be there.
        assert!(file
            .snapshots
            .contains_key(&last_day.format("%Y-%m-%d").to_string()));
        // The oldest seeded date should NOT (it was pruned).
        assert!(!file
            .snapshots
            .contains_key(&start.format("%Y-%m-%d").to_string()));
    }
}
