//! Outline-paste import.
//!
//! Accepts free-form text and tries to identify the 15 Save the Cat beats by
//! header heuristics. Returns a preview the user can edit before committing.
//!
//! Heuristics (in order):
//! 1. Lines that match a known beat label (case-insensitive, with optional
//!    leading markdown headers) become beat headers.
//! 2. Roman/Arabic act numbering ("Act I", "Act 1", "## Act One:") is
//!    recognized but treated as informational.
//! 3. Everything between two beat headers becomes the candidate summary for
//!    the first one.
//!
//! Anything we can't classify is returned in `unmatched` so the user can
//! review it.

use crate::models::structure::BeatId;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ImportedBeat {
    pub id: BeatId,
    pub label: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportPreview {
    pub matched: Vec<ImportedBeat>,
    /// Raw lines that didn't fit anywhere. The UI shows these in a "leftovers"
    /// pane so the user can manually re-route or discard them.
    pub unmatched: Vec<String>,
}

pub fn parse_outline(text: &str) -> ImportPreview {
    let mut matched: Vec<ImportedBeat> = Vec::new();
    let mut unmatched: Vec<String> = Vec::new();
    let mut current_id: Option<BeatId> = None;
    let mut current_buffer: Vec<String> = Vec::new();

    let flush = |id: Option<BeatId>, buf: &mut Vec<String>, matched: &mut Vec<ImportedBeat>| {
        if let Some(id) = id {
            let summary = buf.join("\n").trim().to_string();
            matched.push(ImportedBeat {
                id,
                label: id.label().to_string(),
                summary,
            });
            buf.clear();
        }
    };

    for line in text.lines() {
        let trimmed = strip_markdown_prefix(line.trim());
        if trimmed.is_empty() {
            current_buffer.push(line.to_string());
            continue;
        }
        if let Some(beat) = match_beat_label(trimmed) {
            // Flush previous
            flush(current_id, &mut current_buffer, &mut matched);
            current_id = Some(beat);
            // The same line might contain colon-separated content after the
            // label: "Catalyst: Hero finds the magic letter"
            if let Some(rest) = trimmed.split_once(':').map(|(_, r)| r.trim()) {
                if !rest.is_empty() && !rest.eq_ignore_ascii_case(beat.label()) {
                    current_buffer.push(rest.to_string());
                }
            }
            continue;
        }
        if current_id.is_some() {
            current_buffer.push(line.to_string());
        } else {
            unmatched.push(line.to_string());
        }
    }
    flush(current_id, &mut current_buffer, &mut matched);

    // Trim unmatched: drop leading/trailing blank groups
    while let Some(first) = unmatched.first() {
        if first.trim().is_empty() {
            unmatched.remove(0);
        } else {
            break;
        }
    }
    while let Some(last) = unmatched.last() {
        if last.trim().is_empty() {
            unmatched.pop();
        } else {
            break;
        }
    }

    ImportPreview { matched, unmatched }
}

/// Strip leading markdown header markers and bullet prefixes so we can
/// match the bare label.
fn strip_markdown_prefix(s: &str) -> &str {
    let mut s = s;
    loop {
        let next = s
            .strip_prefix('#')
            .or_else(|| s.strip_prefix("- "))
            .or_else(|| s.strip_prefix("* "))
            .or_else(|| s.strip_prefix("> "))
            .or_else(|| {
                if s.chars().next().is_some_and(|c| c == ' ' || c == '\t') {
                    s.strip_prefix(|c: char| c == ' ' || c == '\t')
                } else {
                    None
                }
            });
        match next {
            Some(n) if n.len() < s.len() => s = n,
            _ => break,
        }
    }
    s.trim()
}

/// Match the start of a line against a beat label, allowing colon-separated
/// content after the label.
fn match_beat_label(s: &str) -> Option<BeatId> {
    let head = s.split(':').next()?.trim();
    let normalized = head.to_lowercase();
    let normalized = normalized.trim_end_matches(|c: char| !c.is_alphanumeric());
    for &id in &BeatId::ALL {
        if id.label().to_lowercase() == normalized {
            return Some(id);
        }
    }
    // Allow "the catalyst", "the midpoint", etc.
    if let Some(rest) = normalized.strip_prefix("the ") {
        for &id in &BeatId::ALL {
            if id.label().to_lowercase() == rest {
                return Some(id);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clean_outline() {
        let text = r#"
Opening Image
A boy sweeps the inn at dawn, watching the road.

Theme Stated
Innkeep: "We choose what we carry."

Catalyst:
A stranger leaves a sealed letter and is gone.

The Midpoint
The kingdom's secret is laid bare; he can't go home.

Final Image
The boy stands at a different door at dawn, looking outward.
"#;
        let prev = parse_outline(text);
        assert_eq!(prev.matched.len(), 5);
        assert_eq!(prev.matched[0].id, BeatId::OpeningImage);
        assert!(prev.matched[0].summary.contains("sweeps the inn"));
        assert_eq!(prev.matched[1].id, BeatId::ThemeStated);
        assert_eq!(prev.matched[2].id, BeatId::Catalyst);
        assert_eq!(prev.matched[3].id, BeatId::Midpoint);
        assert_eq!(prev.matched[4].id, BeatId::FinalImage);
    }

    #[test]
    fn handles_markdown_headers_and_bullets() {
        let text = "## Catalyst\n- letter arrives\n- the boy reads it";
        let prev = parse_outline(text);
        assert_eq!(prev.matched.len(), 1);
        assert_eq!(prev.matched[0].id, BeatId::Catalyst);
        assert!(prev.matched[0].summary.contains("letter arrives"));
    }

    #[test]
    fn unmatched_text_at_top_is_collected() {
        let text = "Some intro that's not a beat\n\nCatalyst\nthe event";
        let prev = parse_outline(text);
        assert_eq!(prev.matched.len(), 1);
        assert!(prev
            .unmatched
            .iter()
            .any(|l| l.contains("intro that's not a beat")));
    }

    #[test]
    fn inline_summary_after_label_colon_is_captured() {
        let text = "Catalyst: A letter arrives at dawn.";
        let prev = parse_outline(text);
        assert_eq!(prev.matched.len(), 1);
        assert_eq!(prev.matched[0].summary.trim(), "A letter arrives at dawn.");
    }
}
