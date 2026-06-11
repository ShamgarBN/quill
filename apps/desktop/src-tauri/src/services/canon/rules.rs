//! Vault sensitivity rules — the privacy guardrail layer.
//!
//! When the user links an Obsidian vault, every file that gets auto- or
//! manually-ingested needs a sensitivity tag (`Public` / `Spoiler` /
//! `DoNotSend`). Three sources resolve it, in priority order:
//!
//!   1. **Frontmatter override** on the individual note.
//!      `---\nquill-sensitivity: do_not_send\n---` wins over everything.
//!   2. **Folder rules** configured per project: a list of `VaultRule`
//!      entries with a folder-name pattern. The first matching rule wins.
//!   3. **Project default** sensitivity for unmatched files (typically
//!      `Public`, but the user can flip it).
//!
//! Patterns are kept intentionally simple to avoid pulling in a glob crate:
//!   - A plain folder name like `DM-Notes` matches if any segment of the
//!     file's relative path equals it (case-sensitive).
//!   - A slash-containing pattern like `Chapter1/DM-Notes` matches if the
//!     relative path STARTS WITH that prefix (with or without a trailing
//!     slash). This lets users scope a rule to a specific subtree.
//!
//! The relative path is computed against the project's `vault_path`. Files
//! outside the vault root fall back to the project default.

use crate::error::Result;
use crate::models::{ChunkSensitivity, VaultRule};
use crate::services::vector::VectorStore;
use std::path::{Path, PathBuf};

/// Resolve the sensitivity for a given on-disk path. Frontmatter wins
/// over folder rules wins over `default`.
pub fn resolve_sensitivity(
    file_path: &Path,
    vault_path: Option<&Path>,
    rules: &[VaultRule],
    default: ChunkSensitivity,
    raw_text: Option<&str>,
) -> ChunkSensitivity {
    // 1. Frontmatter wins.
    if let Some(text) = raw_text {
        if let Some(s) = frontmatter_sensitivity(text) {
            return s;
        }
    }

    // 2. Folder rules — only if we have a vault_path to compute relativity.
    if let Some(vault) = vault_path {
        if let Ok(rel) = file_path.strip_prefix(vault) {
            let rel_str = rel.to_string_lossy();
            for rule in rules {
                if matches_rule(&rule.pattern, &rel_str) {
                    return rule.sensitivity;
                }
            }
        }
    }

    // 3. Default.
    default
}

fn matches_rule(pattern: &str, rel_path: &str) -> bool {
    if pattern.is_empty() {
        return false;
    }
    if pattern.contains('/') {
        // Path-prefix match. Normalize trailing slashes.
        let p = pattern.trim_end_matches('/');
        rel_path == p || rel_path.starts_with(&format!("{p}/"))
    } else {
        // Folder-name match: any path segment equals the pattern.
        rel_path.split('/').any(|seg| seg == pattern)
    }
}

/// Look for a YAML frontmatter `quill-sensitivity` key. Tolerant of
/// whitespace and quoting; accepts the three known string values.
fn frontmatter_sensitivity(text: &str) -> Option<ChunkSensitivity> {
    // Must start with `---` on first line.
    let trimmed = text.trim_start_matches('\u{feff}'); // strip BOM
    if !trimmed.starts_with("---") {
        return None;
    }
    // Walk until the closing `---` on its own line, scanning lines for the key.
    let mut lines = trimmed.lines();
    let _ = lines.next(); // consume opening ---
    for line in lines {
        let line = line.trim_end();
        if line == "---" || line == "..." {
            return None;
        }
        if let Some(rest) = line.strip_prefix("quill-sensitivity:") {
            let value = rest
                .trim()
                .trim_matches(|c: char| c == '\'' || c == '"')
                .to_lowercase();
            return parse_sensitivity(&value);
        }
    }
    None
}

fn parse_sensitivity(v: &str) -> Option<ChunkSensitivity> {
    match v {
        "public" => Some(ChunkSensitivity::Public),
        "spoiler" => Some(ChunkSensitivity::Spoiler),
        "do_not_send" | "do-not-send" | "donotsend" => Some(ChunkSensitivity::DoNotSend),
        _ => None,
    }
}

/// Walk every chunk for `project_id`, resolve sensitivity from
/// `vault_path` + `rules` + `default`, and push the new tags into the
/// vector store. Returns the count of chunks whose tag actually changed.
///
/// NOTE: frontmatter overrides are NOT applied here (that would require
/// re-reading every source file). Frontmatter takes effect on the next
/// ingest of the file.
pub async fn reapply_rules(
    vectors: &dyn VectorStore,
    project_id: &str,
    vault_path: Option<&Path>,
    rules: &[VaultRule],
    default: ChunkSensitivity,
) -> Result<u64> {
    let chunks = vectors.chunks_for_project(project_id).await?;
    let mut updates = Vec::with_capacity(chunks.len());
    for c in chunks {
        if c.source_path.is_empty() {
            // v0.2 chunk without source_path — skip; will get re-tagged on next re-ingest.
            continue;
        }
        let path = PathBuf::from(&c.source_path);
        let new_sens = resolve_sensitivity(&path, vault_path, rules, default, None);
        if new_sens != c.sensitivity {
            updates.push((c.id, new_sens));
        }
    }
    vectors.update_sensitivities(&updates).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn vault() -> PathBuf {
        PathBuf::from("/vault")
    }

    #[test]
    fn folder_name_pattern_matches_anywhere_in_tree() {
        let rules = vec![VaultRule {
            pattern: "DM-Notes".into(),
            sensitivity: ChunkSensitivity::DoNotSend,
        }];
        let p = PathBuf::from("/vault/Chapter1/DM-Notes/session-7.md");
        assert_eq!(
            resolve_sensitivity(&p, Some(&vault()), &rules, ChunkSensitivity::Public, None),
            ChunkSensitivity::DoNotSend
        );
    }

    #[test]
    fn path_prefix_pattern_matches_scoped_subtree() {
        let rules = vec![VaultRule {
            pattern: "Chapter1/DM-Notes".into(),
            sensitivity: ChunkSensitivity::DoNotSend,
        }];
        // In scope.
        let p1 = PathBuf::from("/vault/Chapter1/DM-Notes/session-7.md");
        assert_eq!(
            resolve_sensitivity(&p1, Some(&vault()), &rules, ChunkSensitivity::Public, None),
            ChunkSensitivity::DoNotSend
        );
        // Out of scope (different chapter).
        let p2 = PathBuf::from("/vault/Chapter2/DM-Notes/session-7.md");
        assert_eq!(
            resolve_sensitivity(&p2, Some(&vault()), &rules, ChunkSensitivity::Public, None),
            ChunkSensitivity::Public
        );
    }

    #[test]
    fn first_matching_rule_wins() {
        let rules = vec![
            VaultRule {
                pattern: "Plot".into(),
                sensitivity: ChunkSensitivity::Spoiler,
            },
            VaultRule {
                pattern: "Plot/Twists".into(),
                sensitivity: ChunkSensitivity::DoNotSend,
            },
        ];
        let p = PathBuf::from("/vault/Plot/Twists/big-reveal.md");
        // Plot matches first → Spoiler. (User should order rules carefully.)
        assert_eq!(
            resolve_sensitivity(&p, Some(&vault()), &rules, ChunkSensitivity::Public, None),
            ChunkSensitivity::Spoiler
        );
    }

    #[test]
    fn frontmatter_wins_over_folder_rules() {
        let rules = vec![VaultRule {
            pattern: "Public-Lore".into(),
            sensitivity: ChunkSensitivity::Public,
        }];
        let p = PathBuf::from("/vault/Public-Lore/secret.md");
        let body = "---\nquill-sensitivity: do_not_send\n---\n\nbody here";
        assert_eq!(
            resolve_sensitivity(
                &p,
                Some(&vault()),
                &rules,
                ChunkSensitivity::Public,
                Some(body)
            ),
            ChunkSensitivity::DoNotSend
        );
    }

    #[test]
    fn frontmatter_accepts_quoted_and_dashed_forms() {
        for body in [
            "---\nquill-sensitivity: 'spoiler'\n---\n",
            "---\nquill-sensitivity: \"spoiler\"\n---\n",
        ] {
            assert_eq!(
                frontmatter_sensitivity(body),
                Some(ChunkSensitivity::Spoiler)
            );
        }
        for body in [
            "---\nquill-sensitivity: do-not-send\n---\n",
            "---\nquill-sensitivity: do_not_send\n---\n",
        ] {
            assert_eq!(
                frontmatter_sensitivity(body),
                Some(ChunkSensitivity::DoNotSend)
            );
        }
    }

    #[test]
    fn no_rule_no_frontmatter_falls_back_to_default() {
        let p = PathBuf::from("/vault/Lore/dragons.md");
        assert_eq!(
            resolve_sensitivity(
                &p,
                Some(&vault()),
                &[],
                ChunkSensitivity::Public,
                Some("no frontmatter here")
            ),
            ChunkSensitivity::Public
        );
    }

    #[test]
    fn file_outside_vault_falls_back_to_default() {
        let rules = vec![VaultRule {
            pattern: "Lore".into(),
            sensitivity: ChunkSensitivity::Spoiler,
        }];
        let p = PathBuf::from("/elsewhere/Lore/dragons.md");
        assert_eq!(
            resolve_sensitivity(&p, Some(&vault()), &rules, ChunkSensitivity::Public, None),
            ChunkSensitivity::Public
        );
    }

    #[test]
    fn frontmatter_without_key_returns_none() {
        let body = "---\ntitle: Foo\ntags: [a, b]\n---\nbody";
        assert!(frontmatter_sensitivity(body).is_none());
    }

    #[test]
    fn body_without_frontmatter_returns_none() {
        assert!(frontmatter_sensitivity("just body").is_none());
        assert!(frontmatter_sensitivity("# heading\nbody").is_none());
    }

    #[tokio::test]
    async fn reapply_rules_retags_existing_chunks() {
        use crate::models::CanonChunk;
        use crate::services::vector::{JsonVectorStore, VectorStore};

        let dir = tempfile::tempdir().unwrap();
        let store = JsonVectorStore::open(dir.path().join("v.json")).unwrap();

        let make = |id: &str, path: &str, s: ChunkSensitivity| CanonChunk {
            id: id.into(),
            doc_id: format!("doc_{id}"),
            project_id: "p1".into(),
            index: 0,
            offset: 0,
            text: "x".into(),
            headings: vec![],
            word_count: 1,
            sensitivity: s,
            source_path: path.into(),
            kind: crate::models::CanonKind::Lore,
            embedding_model: String::new(),
        };
        store
            .insert_many(&[
                (
                    make("a", "/vault/Characters/kaelan.md", ChunkSensitivity::Public),
                    vec![1.0, 0.0],
                ),
                (
                    make("b", "/vault/DM-Notes/secrets.md", ChunkSensitivity::Public),
                    vec![0.0, 1.0],
                ),
                (
                    make("c", "/vault/Plot/twists.md", ChunkSensitivity::Public),
                    vec![0.5, 0.5],
                ),
            ])
            .await
            .unwrap();

        let rules = vec![
            VaultRule {
                pattern: "DM-Notes".into(),
                sensitivity: ChunkSensitivity::DoNotSend,
            },
            VaultRule {
                pattern: "Plot".into(),
                sensitivity: ChunkSensitivity::Spoiler,
            },
        ];
        let vault = PathBuf::from("/vault");
        let changed = reapply_rules(&store, "p1", Some(&vault), &rules, ChunkSensitivity::Public)
            .await
            .unwrap();
        assert_eq!(changed, 2); // b and c

        // Verify by walking chunks.
        let chunks = store.chunks_for_project("p1").await.unwrap();
        let by_id: std::collections::HashMap<_, _> = chunks
            .iter()
            .map(|c| (c.id.as_str(), c.sensitivity))
            .collect();
        assert_eq!(by_id["a"], ChunkSensitivity::Public);
        assert_eq!(by_id["b"], ChunkSensitivity::DoNotSend);
        assert_eq!(by_id["c"], ChunkSensitivity::Spoiler);

        // Idempotent — second apply changes nothing.
        let changed2 = reapply_rules(&store, "p1", Some(&vault), &rules, ChunkSensitivity::Public)
            .await
            .unwrap();
        assert_eq!(changed2, 0);
    }
}
