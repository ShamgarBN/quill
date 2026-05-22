//! Cross-link queries: given a character (or any term), find canon
//! chunks and scenes that mention them.
//!
//! Strategy: deterministic case-insensitive substring matching against
//! the character's primary name and aliases. Whole-word boundaries are
//! checked for short terms (≤ 5 chars) to avoid "Ana" matching inside
//! "Banana." Longer names accept any substring hit.
//!
//! Why not embeddings? For a "where is X mentioned" query, the user
//! expects exactness, not semantic similarity. Embeddings are great for
//! "find lore relevant to this scene"; substring matching is the right
//! tool for "find every line that names this character."

use crate::error::Result;
use crate::models::brain::{Character, CrossLink};
use crate::models::canon::CanonChunk;
use crate::models::structure::Scene;
use crate::services::manuscript::ManuscriptStore;
use crate::services::storage::ProjectStore;
use crate::services::structure::StructureStore;
use crate::services::vector::VectorStore;

/// Find every place a character is referenced in the project.
///
/// Inputs:
/// - `character`: the character to look up (by name + aliases)
/// - `projects`: project store, to load scenes + manuscript files
/// - `vectors`: vector store, used here only for `chunks_for_project`
///   (we don't run a similarity search; we walk the chunks)
///
/// Returns matches in a deterministic order: scenes first (by order),
/// then canon chunks (by chunk index).
pub async fn find_cross_links(
    character: &Character,
    projects: &ProjectStore,
    vectors: &dyn VectorStore,
) -> Result<Vec<CrossLink>> {
    let terms: Vec<String> = character
        .match_terms()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if terms.is_empty() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();

    // Scenes -----------------------------------------------------------
    let structure = StructureStore::new(projects);
    let scenes = structure.load_scenes(&character.project_id)?;
    let manuscript = ManuscriptStore::new(projects);
    for scene in &scenes {
        let mut emitted_for_scene = false;
        for term in &terms {
            if scene_metadata_matches(scene, term) {
                let location = match find_metadata_location(scene, term) {
                    Some(loc) => loc,
                    None => "title".to_string(),
                };
                out.push(CrossLink::Scene {
                    scene_id: scene.id.clone(),
                    order: scene.order,
                    title: scene.title.clone(),
                    matched_term: term.clone(),
                    location,
                    snippet: None,
                });
                emitted_for_scene = true;
                break; // one match per scene metadata
            }
        }
        // Body match — load the scene file and search.
        if !emitted_for_scene {
            // Loading every scene file on every cross-link query is
            // affordable for a 90k-word draft (~80 scenes × ~10kb).
            // If this ever grows costly, add a scene-text mention index.
            let content = manuscript.load_scene(&character.project_id, &scene.id, scene.order)?;
            for term in &terms {
                if let Some((idx, _)) = ci_find_with_word_boundary(&content.text, term) {
                    let snippet = excerpt_around(&content.text, idx, term.len(), 160);
                    out.push(CrossLink::Scene {
                        scene_id: scene.id.clone(),
                        order: scene.order,
                        title: scene.title.clone(),
                        matched_term: term.clone(),
                        location: "body".to_string(),
                        snippet: Some(snippet),
                    });
                    break;
                }
            }
        }
    }

    // Canon ------------------------------------------------------------
    let chunks = vectors.chunks_for_project(&character.project_id).await?;
    let mut canon_matches: Vec<&CanonChunk> = Vec::new();
    for chunk in &chunks {
        for term in &terms {
            if let Some(_idx) = ci_find_with_word_boundary(&chunk.text, term) {
                canon_matches.push(chunk);
                break;
            }
        }
    }
    canon_matches.sort_by_key(|c| (c.doc_id.clone(), c.index));
    for chunk in canon_matches {
        for term in &terms {
            if let Some((idx, _)) = ci_find_with_word_boundary(&chunk.text, term) {
                let snippet = excerpt_around(&chunk.text, idx, term.len(), 200);
                out.push(CrossLink::Canon {
                    chunk_id: chunk.id.clone(),
                    doc_id: chunk.doc_id.clone(),
                    matched_term: term.clone(),
                    snippet,
                    headings: chunk.headings.clone(),
                });
                break;
            }
        }
    }

    Ok(out)
}

fn scene_metadata_matches(scene: &Scene, term: &str) -> bool {
    let haystack = [
        scene.title.as_str(),
        scene.pov.as_deref().unwrap_or(""),
        scene.setting.as_deref().unwrap_or(""),
        scene.inciting_incident.as_str(),
        scene.progressive_complication.as_str(),
        scene.crisis.as_str(),
        scene.climax.as_str(),
        scene.resolution.as_str(),
    ];
    haystack
        .iter()
        .any(|s| ci_find_with_word_boundary(s, term).is_some())
}

fn find_metadata_location(scene: &Scene, term: &str) -> Option<String> {
    let pairs: [(&str, &str); 8] = [
        ("title", &scene.title),
        ("pov", scene.pov.as_deref().unwrap_or("")),
        ("setting", scene.setting.as_deref().unwrap_or("")),
        ("inciting_incident", &scene.inciting_incident),
        ("progressive_complication", &scene.progressive_complication),
        ("crisis", &scene.crisis),
        ("climax", &scene.climax),
        ("resolution", &scene.resolution),
    ];
    for (label, body) in pairs {
        if ci_find_with_word_boundary(body, term).is_some() {
            return Some(label.to_string());
        }
    }
    None
}

/// Case-insensitive substring search. For terms of 5 or fewer characters,
/// we require word boundaries on both sides of the match so "Kit" doesn't
/// hit inside "Kitchen." Longer names use plain substring matching.
fn ci_find_with_word_boundary(haystack: &str, needle: &str) -> Option<(usize, usize)> {
    if needle.is_empty() {
        return None;
    }
    let h_lower = haystack.to_lowercase();
    let n_lower = needle.to_lowercase();
    let mut start = 0;
    while let Some(rel) = h_lower[start..].find(&n_lower) {
        let abs = start + rel;
        let end = abs + n_lower.len();
        if needle.chars().count() <= 5 {
            let prev_ok = abs == 0
                || haystack[..abs]
                    .chars()
                    .last()
                    .is_some_and(|c| !c.is_alphanumeric() && c != '\'');
            let next_ok = end >= haystack.len()
                || haystack[end..]
                    .chars()
                    .next()
                    .is_some_and(|c| !c.is_alphanumeric() && c != '\'');
            if prev_ok && next_ok {
                return Some((abs, end));
            }
            start = abs + n_lower.len();
            continue;
        }
        return Some((abs, end));
    }
    None
}

fn excerpt_around(text: &str, match_byte: usize, match_len: usize, window: usize) -> String {
    // Walk back `window/2` chars and forward `window/2` chars from the match.
    let half = window / 2;
    let prefix_chars: Vec<(usize, char)> =
        text[..match_byte].char_indices().rev().take(half).collect();
    let prefix_byte = prefix_chars.last().map(|(i, _)| *i).unwrap_or(match_byte);

    let suffix_byte = text[match_byte + match_len..]
        .char_indices()
        .nth(half)
        .map(|(i, _)| match_byte + match_len + i)
        .unwrap_or(text.len());

    let mut out = String::new();
    if prefix_byte > 0 {
        out.push('…');
    }
    out.push_str(text[prefix_byte..suffix_byte].trim());
    if suffix_byte < text.len() {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ci_find_matches_short_term_at_word_boundary() {
        let text = "Kit drew her dagger. The kitchen was empty.";
        let m = ci_find_with_word_boundary(text, "Kit");
        assert!(m.is_some());
        let (start, _) = m.unwrap();
        assert_eq!(start, 0);
    }

    #[test]
    fn ci_find_short_term_skips_inside_words() {
        let text = "There was a Kitchen and not the boy.";
        // "Kit" should NOT match inside "Kitchen"
        let m = ci_find_with_word_boundary(text, "Kit");
        assert!(m.is_none(), "got {m:?}");
    }

    #[test]
    fn ci_find_long_term_substring_match() {
        let text = "the boy of Tarn";
        let m = ci_find_with_word_boundary(text, "boy of Tarn");
        assert!(m.is_some());
    }

    #[test]
    fn excerpt_clips_with_ellipses() {
        let text =
            "The dragon flew over the lake at dawn, watched by the boy of Tarn from the cliff.";
        let m = ci_find_with_word_boundary(text, "boy of Tarn").unwrap();
        let snippet = excerpt_around(text, m.0, m.1 - m.0, 40);
        assert!(snippet.contains("boy of Tarn"));
        // Heuristic: with a 40-char window we expect at least one ellipsis.
        assert!(snippet.starts_with('…') || snippet.ends_with('…'));
    }

    #[test]
    fn case_insensitive_match() {
        let text = "KAELAN paused. KaElAn smiled.";
        let m = ci_find_with_word_boundary(text, "Kaelan");
        assert!(m.is_some());
    }
}
