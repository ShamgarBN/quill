//! Semantic chunker.
//!
//! Constraints we enforce:
//! - Target chunk size 400–800 *approximate tokens*. We use a fast
//!   tokens≈words heuristic (1 token ≈ 0.75 words for English) and cap by
//!   word count instead of running a real BPE — chunk-size precision is
//!   not load-bearing for retrieval quality, recall sensitivity is.
//! - Respect Markdown headings: never split mid-section if the section is
//!   small enough; never join across H1 boundaries.
//! - Maintain an overlap of N words between adjacent chunks so retrieval
//!   doesn't miss boundary references.
//! - Preserve the source position so we can attribute matches.

use serde::{Deserialize, Serialize};

/// Default to ~600 words/chunk (~800 tokens) with 80 words overlap (~100 tokens).
/// These are conservative for retrieval recall on a worldbuilding corpus.
#[derive(Debug, Clone, Copy)]
pub struct ChunkOptions {
    pub target_words: usize,
    pub max_words: usize,
    pub overlap_words: usize,
}

impl Default for ChunkOptions {
    fn default() -> Self {
        Self {
            target_words: 600,
            max_words: 800,
            overlap_words: 80,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Chunk {
    /// Stable index of this chunk within its source document.
    pub index: u32,
    /// 0-based character offset into the source text where this chunk begins.
    pub offset: u32,
    /// Plain text content of the chunk.
    pub text: String,
    /// Optional heading lineage at this position (e.g. ["The Fallen Empire",
    /// "House of Mirrors"]). Empty for non-Markdown sources.
    pub headings: Vec<String>,
    /// Word count after splitting; for retrieval debugging.
    pub word_count: u32,
}

/// Chunk a plain-text document. No structural awareness; treats input as
/// one long stream and slices it on word boundaries near the target size.
pub fn chunk_plain(text: &str, opts: ChunkOptions) -> Vec<Chunk> {
    chunk_segments(&[Segment::plain(text)], opts)
}

/// Chunk a Markdown document with heading awareness.
///
/// Behavior:
/// - Each heading starts a new logical section; sections smaller than
///   `target_words` may be combined with their following section to reach
///   the target size, but sections larger than `target_words` are split with
///   overlap WITHIN the section (we never duplicate heading-bound content).
/// - Headings are recorded on each emitted chunk as breadcrumb context.
pub fn chunk_markdown(text: &str, opts: ChunkOptions) -> Vec<Chunk> {
    let segments = parse_markdown_sections(text);
    chunk_segments(&segments, opts)
}

// ---------- internals ----------

#[derive(Debug, Clone)]
struct Segment {
    /// Heading lineage at the start of this segment.
    headings: Vec<String>,
    /// Character offset in the original document where this segment begins.
    offset: u32,
    /// Body text of the segment (without the heading line).
    body: String,
}

impl Segment {
    fn plain(text: &str) -> Self {
        Self {
            headings: Vec::new(),
            offset: 0,
            body: text.to_string(),
        }
    }
}

/// Parse Markdown into heading-bounded segments. We use a hand-rolled
/// scanner — no full Markdown AST is needed, we only care about ATX headings
/// (`#`, `##`, ...).
fn parse_markdown_sections(text: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut current_headings: Vec<String> = Vec::new();
    let mut current_body = String::new();
    let mut current_offset: u32 = 0;
    let mut cursor: u32 = 0;
    let mut in_code_fence = false;

    let push_segment =
        |segs: &mut Vec<Segment>, headings: &[String], offset: u32, body: &mut String| {
            if !body.trim().is_empty() {
                segs.push(Segment {
                    headings: headings.to_vec(),
                    offset,
                    body: std::mem::take(body),
                });
            }
        };

    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        // Toggle code fences so we don't treat ``` # ``` as a heading
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code_fence = !in_code_fence;
            current_body.push_str(line);
            cursor += line.len() as u32;
            continue;
        }
        if !in_code_fence && trimmed.starts_with('#') {
            if let Some((depth, title)) = parse_atx_heading(trimmed) {
                // Flush whatever we'd accumulated into a segment of the prior heading.
                push_segment(
                    &mut segments,
                    &current_headings,
                    current_offset,
                    &mut current_body,
                );

                // Truncate breadcrumb to this depth and push the new title.
                current_headings.truncate(depth.saturating_sub(1));
                current_headings.push(title);
                current_offset = cursor + line.len() as u32;
                cursor += line.len() as u32;
                continue;
            }
        }
        current_body.push_str(line);
        cursor += line.len() as u32;
    }
    push_segment(
        &mut segments,
        &current_headings,
        current_offset,
        &mut current_body,
    );

    if segments.is_empty() {
        // Document had no headings (or was empty)
        if !text.trim().is_empty() {
            segments.push(Segment {
                headings: Vec::new(),
                offset: 0,
                body: text.to_string(),
            });
        }
    }
    segments
}

/// Recognize an ATX heading line. Returns (depth, title) on match.
fn parse_atx_heading(s: &str) -> Option<(usize, String)> {
    let mut depth = 0usize;
    let bytes = s.as_bytes();
    while depth < bytes.len() && bytes[depth] == b'#' {
        depth += 1;
    }
    if depth == 0 || depth > 6 {
        return None;
    }
    if depth >= bytes.len() {
        return None;
    }
    if bytes[depth] != b' ' {
        return None;
    }
    let title = std::str::from_utf8(&bytes[depth + 1..])
        .ok()?
        .trim_end_matches('\n')
        .trim_end_matches('\r')
        .trim()
        .trim_end_matches('#')
        .trim()
        .to_string();
    if title.is_empty() {
        return None;
    }
    Some((depth, title))
}

fn chunk_segments(segments: &[Segment], opts: ChunkOptions) -> Vec<Chunk> {
    let mut out = Vec::new();
    let mut next_index: u32 = 0;

    for seg in segments {
        let words = collect_words(&seg.body);
        if words.is_empty() {
            continue;
        }

        // Small enough to be a single chunk.
        if words.len() <= opts.max_words {
            out.push(make_chunk(
                next_index,
                seg.offset,
                &seg.headings,
                &seg.body,
                &words,
                0,
                words.len(),
            ));
            next_index += 1;
            continue;
        }

        // Too large: slice into overlapping windows.
        let mut start = 0usize;
        while start < words.len() {
            let end = (start + opts.target_words).min(words.len());
            out.push(make_chunk(
                next_index,
                seg.offset,
                &seg.headings,
                &seg.body,
                &words,
                start,
                end,
            ));
            next_index += 1;
            if end == words.len() {
                break;
            }
            start = end.saturating_sub(opts.overlap_words);
        }
    }

    out
}

/// Word with byte-offset relative to the segment body.
#[derive(Debug, Clone, Copy)]
struct Word {
    start: usize,
    end: usize,
}

fn collect_words(text: &str) -> Vec<Word> {
    let mut words = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        // Skip whitespace
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        words.push(Word { start, end: i });
    }
    words
}

fn make_chunk(
    index: u32,
    seg_offset: u32,
    headings: &[String],
    body: &str,
    words: &[Word],
    start: usize,
    end: usize,
) -> Chunk {
    let from = words[start].start;
    let to = words[end - 1].end;
    let slice = &body[from..to];
    Chunk {
        index,
        offset: seg_offset + from as u32,
        text: slice.trim().to_string(),
        headings: headings.to_vec(),
        word_count: (end - start) as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(target: usize, max: usize, overlap: usize) -> ChunkOptions {
        ChunkOptions {
            target_words: target,
            max_words: max,
            overlap_words: overlap,
        }
    }

    #[test]
    fn empty_input_yields_no_chunks() {
        assert!(chunk_plain("", ChunkOptions::default()).is_empty());
        assert!(chunk_plain("   \n  ", ChunkOptions::default()).is_empty());
    }

    #[test]
    fn small_input_yields_single_chunk() {
        let text = "The dragon awoke at dawn. The valley fell silent.";
        let chunks = chunk_plain(text, ChunkOptions::default());
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].word_count, 9);
        assert_eq!(chunks[0].index, 0);
        assert_eq!(chunks[0].offset, 0);
    }

    #[test]
    fn large_input_splits_with_overlap() {
        let words: Vec<String> = (0..1500).map(|i| format!("w{i}")).collect();
        let text = words.join(" ");
        let chunks = chunk_plain(&text, opts(400, 600, 50));
        assert!(chunks.len() >= 3);
        // Indices are sequential
        for (i, c) in chunks.iter().enumerate() {
            assert_eq!(c.index, i as u32);
        }
        // Verify overlap: last 50 words of chunk N appear as first 50 of N+1
        for w in chunks.windows(2) {
            let prev_words: Vec<&str> = w[0].text.split_whitespace().collect();
            let next_words: Vec<&str> = w[1].text.split_whitespace().collect();
            let overlap_size = 50;
            let prev_tail = &prev_words[prev_words.len() - overlap_size..];
            let next_head = &next_words[..overlap_size];
            assert_eq!(
                prev_tail, next_head,
                "overlap mismatch between adjacent chunks"
            );
        }
    }

    #[test]
    fn markdown_headings_become_breadcrumbs() {
        let md = "# World\n\nIntro text here.\n\n## Kingdoms\n\nThe northern kingdom.\n\n### Lake Tarn\n\nDetails about the lake.\n";
        let chunks = chunk_markdown(md, ChunkOptions::default());
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].headings, vec!["World"]);
        assert_eq!(chunks[1].headings, vec!["World", "Kingdoms"]);
        assert_eq!(chunks[2].headings, vec!["World", "Kingdoms", "Lake Tarn"]);
    }

    #[test]
    fn code_fences_are_not_parsed_as_headings() {
        let md = "# Real Heading\n\nbody\n\n```\n# This is just code\n```\n\nmore body\n";
        let chunks = chunk_markdown(md, ChunkOptions::default());
        // Single section under "Real Heading" — the fenced "# This is just code"
        // must NOT have created a new section.
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].headings, vec!["Real Heading"]);
        assert!(chunks[0].text.contains("# This is just code"));
    }

    #[test]
    fn parse_atx_handles_trailing_hashes_and_spaces() {
        assert_eq!(
            parse_atx_heading("## Title ##"),
            Some((2, "Title".to_string()))
        );
        assert_eq!(
            parse_atx_heading("#### Deep ##  "),
            Some((4, "Deep".to_string()))
        );
        assert_eq!(parse_atx_heading("#NoSpace"), None);
        assert_eq!(parse_atx_heading("####### TooDeep"), None);
        assert_eq!(parse_atx_heading("# "), None);
    }

    #[test]
    fn offsets_are_monotonic_and_within_bounds() {
        let text = "alpha beta gamma delta epsilon zeta eta theta iota kappa".repeat(50);
        let chunks = chunk_plain(&text, opts(50, 80, 10));
        let mut last_offset = 0u32;
        for c in &chunks {
            assert!(
                (c.offset as usize) <= text.len(),
                "offset out of bounds: {}",
                c.offset
            );
            assert!(c.offset >= last_offset);
            last_offset = c.offset;
        }
    }
}
