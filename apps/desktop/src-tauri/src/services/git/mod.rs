//! Per-project Git auto-commit.
//!
//! Each project gets its own local-only repository at
//! `<data_dir>/projects/<id>/.git/`. No remote is configured. Every save
//! produces a commit with a sensible default message (or a user-supplied
//! one). Full history is preserved; rollback is `git checkout <oid>`.
//!
//! Implementation: we shell out to the system `git` binary. macOS ships it
//! via Xcode CLT (a documented prerequisite). This is dramatically simpler
//! and more stable than wiring up a Rust-native git library, with the only
//! cost being a single subprocess per commit — negligible at hobby pace.

use crate::error::{QuillError, Result};
use crate::models::CommitInfo;
use chrono::TimeZone;
use std::path::{Path, PathBuf};
use std::process::Command;

const DEFAULT_AUTHOR_NAME: &str = "Quill";
const DEFAULT_AUTHOR_EMAIL: &str = "quill@localhost";

pub struct GitService {
    project_dir: PathBuf,
}

impl GitService {
    pub fn for_project(project_dir: &Path) -> Self {
        Self {
            project_dir: project_dir.to_path_buf(),
        }
    }

    fn git(&self) -> Command {
        let mut c = Command::new("git");
        c.arg("-C").arg(&self.project_dir);
        c
    }

    /// Initialize the repo if it doesn't exist yet. Idempotent.
    fn ensure_repo(&self) -> Result<()> {
        if self.project_dir.join(".git").exists() {
            return Ok(());
        }
        std::fs::create_dir_all(&self.project_dir)?;
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.project_dir)
            .args(["init", "--initial-branch=main"])
            .output()
            .map_err(|e| QuillError::Git(format!("git init failed to spawn: {e}")))?;
        if !out.status.success() {
            return Err(QuillError::Git(format!(
                "git init failed: {}",
                String::from_utf8_lossy(&out.stderr)
            )));
        }
        Ok(())
    }

    /// Stage everything in the working tree and create a commit.
    ///
    /// Returns `None` if there were no changes to commit.
    pub fn commit_all(&self, message: Option<&str>) -> Result<Option<CommitInfo>> {
        self.ensure_repo()?;
        ensure_local_identity(&self.project_dir)?;

        // Stage everything
        let add = self
            .git()
            .args(["add", "-A"])
            .output()
            .map_err(|e| QuillError::Git(format!("git add failed to spawn: {e}")))?;
        if !add.status.success() {
            return Err(QuillError::Git(format!(
                "git add failed: {}",
                String::from_utf8_lossy(&add.stderr)
            )));
        }

        // Check for staged changes
        let diff = self
            .git()
            .args(["diff", "--cached", "--name-only"])
            .output()
            .map_err(|e| QuillError::Git(format!("git diff failed to spawn: {e}")))?;
        if !diff.status.success() {
            return Err(QuillError::Git(format!(
                "git diff --cached failed: {}",
                String::from_utf8_lossy(&diff.stderr)
            )));
        }
        let changed_count = diff
            .stdout
            .split(|&b| b == b'\n')
            .filter(|l| !l.is_empty())
            .count() as u32;
        if changed_count == 0 {
            return Ok(None);
        }

        // Compose the commit message
        let auto = format!(
            "save: {} files at {}",
            changed_count,
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
        );
        let msg = message.unwrap_or(&auto);

        // Commit
        let commit = self
            .git()
            .args(["commit", "-m", msg])
            .output()
            .map_err(|e| QuillError::Git(format!("git commit failed to spawn: {e}")))?;
        if !commit.status.success() {
            return Err(QuillError::Git(format!(
                "git commit failed: {}",
                String::from_utf8_lossy(&commit.stderr)
            )));
        }

        // Get the new HEAD oid
        let rev = self
            .git()
            .args(["rev-parse", "HEAD"])
            .output()
            .map_err(|e| QuillError::Git(format!("git rev-parse failed to spawn: {e}")))?;
        if !rev.status.success() {
            return Err(QuillError::Git(format!(
                "git rev-parse HEAD failed: {}",
                String::from_utf8_lossy(&rev.stderr)
            )));
        }
        let oid = String::from_utf8(rev.stdout)?.trim().to_string();
        let short_oid = oid.chars().take(7).collect::<String>();

        Ok(Some(CommitInfo {
            oid,
            short_oid,
            message: msg.to_string(),
            timestamp: chrono::Utc::now(),
            files_changed: changed_count,
        }))
    }

    /// List the most recent commits.
    ///
    /// Phase 0 implementation shells out to `git log`. We avoid pulling in
    /// gitoxide's traversal API surface here because it changes between
    /// minor versions; shelling keeps the implementation stable.
    pub fn log(&self, limit: usize) -> Result<Vec<CommitInfo>> {
        if !self.project_dir.join(".git").exists() {
            return Ok(Vec::new());
        }
        // %H = full hash, %s = subject, %ct = committer date (unix seconds)
        let format = "%H%x09%ct%x09%s";
        let out = self
            .git()
            .args([
                "log",
                &format!("--max-count={}", limit),
                &format!("--pretty=format:{}", format),
            ])
            .output()
            .map_err(|e| QuillError::Git(format!("git log failed to spawn: {e}")))?;
        if !out.status.success() {
            // No commits yet results in non-zero exit; treat as empty.
            return Ok(Vec::new());
        }
        let stdout = String::from_utf8(out.stdout)?;
        let mut commits = Vec::new();
        for line in stdout.lines() {
            let parts: Vec<&str> = line.splitn(3, '\t').collect();
            if parts.len() != 3 {
                continue;
            }
            let oid = parts[0].to_string();
            let short_oid = oid.chars().take(7).collect::<String>();
            let ts_secs: i64 = parts[1].parse().unwrap_or(0);
            let ts = chrono::Utc
                .timestamp_opt(ts_secs, 0)
                .single()
                .unwrap_or_else(chrono::Utc::now);
            commits.push(CommitInfo {
                oid,
                short_oid,
                message: parts[2].to_string(),
                timestamp: ts,
                files_changed: 0,
            });
        }
        Ok(commits)
    }
}

/// Set local-only `user.name` and `user.email` for this repo. Idempotent.
fn ensure_local_identity(project_dir: &Path) -> Result<()> {
    set_local_config(project_dir, "user.name", DEFAULT_AUTHOR_NAME)?;
    set_local_config(project_dir, "user.email", DEFAULT_AUTHOR_EMAIL)?;
    // Defensively ignore commit signing for the project repo: a globally-
    // configured GPG/SSH signing key shouldn't fail Quill saves.
    set_local_config(project_dir, "commit.gpgsign", "false")?;
    Ok(())
}

fn set_local_config(project_dir: &Path, key: &str, value: &str) -> Result<()> {
    let out = Command::new("git")
        .arg("-C")
        .arg(project_dir)
        .args(["config", "--local", key, value])
        .output()
        .map_err(|e| QuillError::Git(format!("git config failed to spawn: {e}")))?;
    if !out.status.success() {
        return Err(QuillError::Git(format!(
            "git config {key} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(p: &Path, content: &str) {
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, content).unwrap();
    }

    #[test]
    fn first_commit_creates_history() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("manuscript/00-front-matter.md"),
            "# Working draft\n",
        );
        let svc = GitService::for_project(dir.path());
        let info = svc.commit_all(Some("initial")).unwrap().expect("commit");
        assert!(info.short_oid.len() == 7);
        assert!(info.files_changed >= 1);
        assert_eq!(info.message, "initial");
    }

    #[test]
    fn no_changes_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("a.md"), "x");
        let svc = GitService::for_project(dir.path());
        svc.commit_all(Some("first")).unwrap().expect("commit");
        let again = svc.commit_all(Some("noop")).unwrap();
        assert!(again.is_none());
    }

    #[test]
    fn log_returns_recent_commits() {
        let dir = tempfile::tempdir().unwrap();
        let svc = GitService::for_project(dir.path());
        write(&dir.path().join("a.md"), "1");
        svc.commit_all(Some("first")).unwrap();
        write(&dir.path().join("a.md"), "2");
        svc.commit_all(Some("second")).unwrap();
        write(&dir.path().join("a.md"), "3");
        svc.commit_all(Some("third")).unwrap();
        let log = svc.log(10).unwrap();
        assert_eq!(log.len(), 3);
        assert_eq!(log[0].message, "third");
        assert_eq!(log[2].message, "first");
    }
}
