//! Feature extraction. Pure function over an &str.

use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

/// Top-44 English function words used as a per-text frequency vector. Order
/// is stable — must not change without bumping the fingerprint version, since
/// stored fingerprints index into this array.
pub const FUNCTION_WORDS: [&str; 44] = [
    "the", "and", "of", "to", "a", "in", "that", "is", "was", "it", "for", "with", "as", "his",
    "her", "he", "she", "they", "we", "you", "but", "be", "this", "have", "had", "from", "or",
    "not", "on", "at", "by", "an", "if", "would", "will", "what", "when", "so", "no", "all",
    "their", "him", "out", "into",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceFeatures {
    pub sentence_count: u32,
    pub word_count: u32,
    pub char_count: u32,

    pub mean_sentence_words: f32,
    pub stddev_sentence_words: f32,
    pub median_sentence_words: f32,
    pub p10_sentence_words: f32,
    pub p90_sentence_words: f32,
    pub fragment_ratio: f32,
    pub long_sentence_ratio: f32,

    pub type_token_ratio: f32,
    pub mean_word_length: f32,

    pub dialogue_ratio: f32,
    pub dialogue_tag_density: f32,

    pub comma_density: f32,
    pub emdash_density: f32,
    pub semicolon_density: f32,
    pub colon_density: f32,
    pub paren_density: f32,
    pub period_pct: f32,
    pub bang_pct: f32,
    pub question_pct: f32,
    pub ellipsis_pct: f32,
    pub emdash_end_pct: f32,

    /// Frequency of each FUNCTION_WORDS entry, normalized by word_count.
    pub function_word_freq: Vec<f32>,
}

/// Number of dimensions when flattened into a feature vector for comparison.
pub const FEATURE_DIMENSIONS: usize = 22 + 44;

/// Stable labels matching the order of `to_vector()`. Used by the UI to
/// label a "drift bars" chart without coupling to field names.
pub const FEATURE_LABELS: &[&str] = &[
    "mean sentence",
    "stddev sentence",
    "median sentence",
    "p10 sentence",
    "p90 sentence",
    "fragment ratio",
    "long sentence ratio",
    "type/token",
    "mean word length",
    "dialogue ratio",
    "dialogue tags",
    "comma density",
    "em-dash density",
    "semicolon density",
    "colon density",
    "paren density",
    "period %",
    "! %",
    "? %",
    "ellipsis %",
    "em-dash end %",
    "—reserved—",
];

impl VoiceFeatures {
    /// Flatten into a comparable vector. Order MUST match FEATURE_LABELS for
    /// the first 22 entries; the last 44 are FUNCTION_WORDS in canonical order.
    pub fn to_vector(&self) -> Vec<f32> {
        let mut v = Vec::with_capacity(FEATURE_DIMENSIONS);
        v.push(self.mean_sentence_words);
        v.push(self.stddev_sentence_words);
        v.push(self.median_sentence_words);
        v.push(self.p10_sentence_words);
        v.push(self.p90_sentence_words);
        v.push(self.fragment_ratio);
        v.push(self.long_sentence_ratio);
        v.push(self.type_token_ratio);
        v.push(self.mean_word_length);
        v.push(self.dialogue_ratio);
        v.push(self.dialogue_tag_density);
        v.push(self.comma_density);
        v.push(self.emdash_density);
        v.push(self.semicolon_density);
        v.push(self.colon_density);
        v.push(self.paren_density);
        v.push(self.period_pct);
        v.push(self.bang_pct);
        v.push(self.question_pct);
        v.push(self.ellipsis_pct);
        v.push(self.emdash_end_pct);
        v.push(0.0); // reserved
        v.extend_from_slice(&self.function_word_freq);
        debug_assert_eq!(v.len(), FEATURE_DIMENSIONS);
        v
    }
}

pub fn extract_features(text: &str) -> VoiceFeatures {
    let normalized = normalize(text);
    let chars = normalized.chars().count() as u32;
    let words = words_lower(&normalized);
    let word_count = words.len() as u32;

    let sentences = split_sentences(&normalized);
    let sentence_lens: Vec<usize> = sentences
        .iter()
        .map(|s| count_words(s))
        .filter(|n| *n > 0)
        .collect();
    let sentence_count = sentence_lens.len() as u32;

    let (mean, stddev) = mean_stddev(&sentence_lens);
    let (median, p10, p90) = quantiles(&sentence_lens);

    let frag = sentence_lens.iter().filter(|&&n| n <= 4 && n > 0).count();
    let long = sentence_lens.iter().filter(|&&n| n >= 30).count();
    let fragment_ratio = ratio(frag, sentence_lens.len());
    let long_sentence_ratio = ratio(long, sentence_lens.len());

    let unique_word_types: std::collections::HashSet<&str> =
        words.iter().map(|s| s.as_str()).collect();
    let type_token_ratio = ratio(unique_word_types.len(), word_count as usize);
    let total_chars: usize = words.iter().map(|w| w.chars().count()).sum();
    let mean_word_length = if word_count == 0 {
        0.0
    } else {
        total_chars as f32 / word_count as f32
    };

    let (dialogue_chars, dialogue_tag_count) = analyze_dialogue(&normalized);
    let dialogue_ratio = if chars == 0 {
        0.0
    } else {
        dialogue_chars as f32 / chars as f32
    };
    let dialogue_tag_density = if sentence_count == 0 {
        0.0
    } else {
        dialogue_tag_count as f32 / sentence_count as f32
    };

    let comma = count_byte(&normalized, b',');
    let emdash = count_substr(&normalized, "—") + count_substr(&normalized, "--");
    let semi = count_byte(&normalized, b';');
    let colon = count_byte(&normalized, b':');
    let paren = count_byte(&normalized, b'(') + count_byte(&normalized, b')');

    let per100 = |n: usize| -> f32 {
        if word_count == 0 {
            0.0
        } else {
            n as f32 * 100.0 / word_count as f32
        }
    };

    let comma_density = per100(comma);
    let emdash_density = per100(emdash);
    let semicolon_density = per100(semi);
    let colon_density = per100(colon);
    let paren_density = per100(paren);

    // Sentence-end distribution
    let mut period = 0;
    let mut bang = 0;
    let mut question = 0;
    let mut ellipsis = 0;
    let mut em_end = 0;
    for s in &sentences {
        let trimmed = s
            .trim_end()
            .trim_end_matches(['"', '”', '\'', ')', ']'].as_ref());
        if trimmed.ends_with("...") || trimmed.ends_with('…') {
            ellipsis += 1;
        } else if trimmed.ends_with("—") || trimmed.ends_with("--") {
            em_end += 1;
        } else if trimmed.ends_with('!') {
            bang += 1;
        } else if trimmed.ends_with('?') {
            question += 1;
        } else if trimmed.ends_with('.') {
            period += 1;
        }
    }
    let total_endings = (period + bang + question + ellipsis + em_end).max(1);
    let pct = |n: usize| -> f32 { n as f32 * 100.0 / total_endings as f32 };

    // Function word frequencies
    let mut fw_counts = vec![0u32; FUNCTION_WORDS.len()];
    let mut fw_index: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (i, w) in FUNCTION_WORDS.iter().enumerate() {
        fw_index.insert(*w, i);
    }
    for w in &words {
        if let Some(&i) = fw_index.get(w.as_str()) {
            fw_counts[i] += 1;
        }
    }
    let function_word_freq: Vec<f32> = fw_counts
        .iter()
        .map(|&c| {
            if word_count == 0 {
                0.0
            } else {
                c as f32 / word_count as f32
            }
        })
        .collect();

    VoiceFeatures {
        sentence_count,
        word_count,
        char_count: chars,
        mean_sentence_words: mean,
        stddev_sentence_words: stddev,
        median_sentence_words: median,
        p10_sentence_words: p10,
        p90_sentence_words: p90,
        fragment_ratio,
        long_sentence_ratio,
        type_token_ratio,
        mean_word_length,
        dialogue_ratio,
        dialogue_tag_density,
        comma_density,
        emdash_density,
        semicolon_density,
        colon_density,
        paren_density,
        period_pct: pct(period),
        bang_pct: pct(bang),
        question_pct: pct(question),
        ellipsis_pct: pct(ellipsis),
        emdash_end_pct: pct(em_end),
        function_word_freq,
    }
}

// ---------- helpers ----------

/// Replace curly quotes with straight ones, normalize whitespace, and
/// preserve paragraph structure. Lossless at the level we care about.
fn normalize(text: &str) -> String {
    text.replace(['“', '”'], "\"").replace(['‘', '’'], "'")
}

/// Split into sentence-like fragments. We use a hand-rolled splitter
/// because Rust doesn't ship one and pulling a full NLP crate is overkill;
/// for our voice-feature scope, this approximation is plenty.
fn split_sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        cur.push(c);
        let is_terminator = c == '.' || c == '!' || c == '?' || c == '…';
        if is_terminator {
            // Don't split on common abbreviations: Mr. Mrs. Dr. Ms. St. e.g. i.e.
            if c == '.' && looks_like_abbrev(&cur) {
                i += 1;
                continue;
            }
            // Consume trailing closing quotes/parens
            let mut j = i + 1;
            while j < chars.len() && matches!(chars[j], '"' | ')' | ']' | '\'') {
                cur.push(chars[j]);
                j += 1;
            }
            i = j;
            // Sentence ends at next whitespace
            while i < chars.len() && chars[i].is_whitespace() {
                cur.push(chars[i]);
                i += 1;
            }
            out.push(std::mem::take(&mut cur));
            continue;
        }
        i += 1;
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

fn looks_like_abbrev(s: &str) -> bool {
    let trimmed = s.trim_end_matches('.');
    let last_word = trimmed
        .rsplit(|c: char| c.is_whitespace())
        .next()
        .unwrap_or("");
    matches!(
        last_word,
        "Mr" | "Mrs" | "Ms" | "Dr" | "St" | "Sr" | "Jr" | "vs" | "etc"
    )
}

fn count_words(s: &str) -> usize {
    s.unicode_words().count()
}

fn words_lower(s: &str) -> Vec<String> {
    s.unicode_words()
        .map(|w| w.to_lowercase())
        .filter(|w| !w.is_empty())
        .collect()
}

fn count_byte(s: &str, b: u8) -> usize {
    s.bytes().filter(|x| *x == b).count()
}

fn count_substr(s: &str, sub: &str) -> usize {
    if sub.is_empty() {
        return 0;
    }
    s.matches(sub).count()
}

fn ratio(num: usize, denom: usize) -> f32 {
    if denom == 0 {
        0.0
    } else {
        num as f32 / denom as f32
    }
}

fn mean_stddev(xs: &[usize]) -> (f32, f32) {
    if xs.is_empty() {
        return (0.0, 0.0);
    }
    let n = xs.len() as f32;
    let mean = xs.iter().map(|&x| x as f32).sum::<f32>() / n;
    let var = xs
        .iter()
        .map(|&x| {
            let d = x as f32 - mean;
            d * d
        })
        .sum::<f32>()
        / n;
    (mean, var.sqrt())
}

fn quantiles(xs: &[usize]) -> (f32, f32, f32) {
    if xs.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let mut sorted: Vec<f32> = xs.iter().map(|&x| x as f32).collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let q = |p: f32| -> f32 {
        let idx = ((sorted.len() - 1) as f32 * p).round() as usize;
        sorted[idx.min(sorted.len() - 1)]
    };
    (q(0.5), q(0.10), q(0.90))
}

/// Returns (chars_in_quotes, dialogue_tag_count).
///
/// Detects `"…", X said` and `"…!" cried Y` style patterns.
fn analyze_dialogue(text: &str) -> (usize, usize) {
    let mut in_q = false;
    let mut quoted = 0usize;
    let mut tag = 0usize;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' {
            if in_q {
                // close quote; check for dialogue tag right after
                let rest = &text[i + 1..];
                if matches_dialogue_tag(rest) {
                    tag += 1;
                }
                in_q = false;
            } else {
                in_q = true;
            }
            i += 1;
            continue;
        }
        if in_q {
            quoted += 1;
        }
        i += 1;
    }
    (quoted, tag)
}

fn matches_dialogue_tag(rest: &str) -> bool {
    // Allow optional comma/period/exclamation before the tag
    let trimmed = rest.trim_start_matches([',', '.', '!', '?']).trim_start();
    let lower = trimmed.to_ascii_lowercase();
    const TAG_VERBS: &[&str] = &[
        "said",
        "asked",
        "replied",
        "whispered",
        "muttered",
        "shouted",
        "growled",
        "called",
        "cried",
        "shrugged",
        "answered",
        "snapped",
        "yelled",
        "murmured",
        "agreed",
        "added",
    ];
    for v in TAG_VERBS {
        if lower.starts_with(v) {
            // word boundary
            let next = lower.as_bytes().get(v.len()).copied().unwrap_or(b' ');
            if !next.is_ascii_alphanumeric() {
                return true;
            }
        }
        // Pattern "<Name> said" — accept "<Word> <verb>"
        if let Some(idx) = lower.find(v) {
            if idx > 0
                && idx < 40
                && lower.as_bytes()[idx - 1] == b' '
                && lower.as_bytes()[..idx]
                    .iter()
                    .all(|&c| c.is_ascii_alphanumeric() || c == b' ' || c == b'\'' || c == b'-')
            {
                let next = lower.as_bytes().get(idx + v.len()).copied().unwrap_or(b' ');
                if !next.is_ascii_alphanumeric() {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_yields_zero_features() {
        let f = extract_features("");
        assert_eq!(f.sentence_count, 0);
        assert_eq!(f.word_count, 0);
        let v = f.to_vector();
        assert_eq!(v.len(), FEATURE_DIMENSIONS);
        assert!(v.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn sentence_lengths_and_quantiles() {
        let text = "The dragon flew. It was vast. The boy watched in silence as the wings caught the fading light. Quiet.";
        let f = extract_features(text);
        assert_eq!(f.sentence_count, 4);
        // mean of [3,3,12,1] = 4.75
        assert!(
            (f.mean_sentence_words - 4.75).abs() < 0.5,
            "mean={}",
            f.mean_sentence_words
        );
        // fragment ratio: 3 of 4 sentences are ≤4 words
        assert!(f.fragment_ratio >= 0.5);
    }

    #[test]
    fn function_word_frequencies_are_normalized() {
        // 6 occurrences of "the" out of 11 words → 6/11 ≈ 0.5454
        let text = "the dragon and the boy and the lake the the the";
        let f = extract_features(text);
        let the_idx = FUNCTION_WORDS.iter().position(|w| *w == "the").unwrap();
        let r = f.function_word_freq[the_idx];
        assert!(
            (r - 6.0 / 11.0).abs() < 0.01,
            "expected the/total ≈ 6/11, got {r}"
        );
    }

    #[test]
    fn dialogue_ratio_and_tags() {
        let text = "\"Hello,\" she said. \"How are you?\" asked the man. He shrugged.";
        let f = extract_features(text);
        assert!(f.dialogue_ratio > 0.0);
        assert!(f.dialogue_tag_density > 0.0);
    }

    #[test]
    fn punctuation_densities_per_100_words() {
        let text = "One, two, three, four; five — six. Seven (eight) nine: ten.";
        let f = extract_features(text);
        // 3 commas in ~10 words = 30 per 100
        assert!(f.comma_density >= 25.0, "got {}", f.comma_density);
        assert!(f.emdash_density > 0.0);
        assert!(f.semicolon_density > 0.0);
        assert!(f.colon_density > 0.0);
        assert!(f.paren_density > 0.0);
    }

    #[test]
    fn sentence_end_distribution_sums_to_100() {
        let text = "Hello. Hello! Hello? Goodbye…";
        let f = extract_features(text);
        let sum = f.period_pct + f.bang_pct + f.question_pct + f.ellipsis_pct + f.emdash_end_pct;
        assert!((sum - 100.0).abs() < 1.0, "sum should ≈ 100, got {sum}");
        assert!(f.bang_pct > 0.0);
        assert!(f.question_pct > 0.0);
        assert!(f.ellipsis_pct > 0.0);
    }

    #[test]
    fn abbreviations_dont_split_sentences() {
        let text = "Dr. Smith knocked. Then he waited.";
        let f = extract_features(text);
        assert_eq!(f.sentence_count, 2);
    }

    #[test]
    fn vector_length_matches_constant() {
        let f = extract_features("Hello world. Another sentence.");
        assert_eq!(f.to_vector().len(), FEATURE_DIMENSIONS);
    }
}
