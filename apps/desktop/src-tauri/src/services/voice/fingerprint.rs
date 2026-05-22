//! Voice fingerprint = centroid of feature vectors weighted by passage size.
//!
//! Drift = signed distance per-feature between a candidate passage and the
//! reference fingerprint, plus an overall L2 + cosine score.

use crate::services::voice::extractor::{
    extract_features, VoiceFeatures, FEATURE_DIMENSIONS, FEATURE_LABELS, FUNCTION_WORDS,
};
use serde::{Deserialize, Serialize};

/// Stable representation of "the user's voice."
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceFingerprint {
    /// Mean per-feature value across reference passages (length = FEATURE_DIMENSIONS).
    pub mean: Vec<f32>,
    /// Std dev per feature; used to scale drift to z-scores.
    pub stddev: Vec<f32>,
    /// Number of passages aggregated.
    pub passage_count: u32,
    /// Total words behind the centroid — the larger this is, the more
    /// confident we are.
    pub total_words: u32,
}

impl VoiceFingerprint {
    pub fn empty() -> Self {
        Self {
            mean: vec![0.0; FEATURE_DIMENSIONS],
            stddev: vec![0.0; FEATURE_DIMENSIONS],
            passage_count: 0,
            total_words: 0,
        }
    }
}

/// Build a weighted fingerprint from a set of passages. Weight is `word_count`
/// so a 2k-word excerpt counts more than a 200-word snippet.
pub fn build_fingerprint(features: &[VoiceFeatures]) -> VoiceFingerprint {
    if features.is_empty() {
        return VoiceFingerprint::empty();
    }

    // Weighted mean
    let total_words: u64 = features.iter().map(|f| f.word_count as u64).sum();
    if total_words == 0 {
        return VoiceFingerprint::empty();
    }
    let mut mean = vec![0.0f32; FEATURE_DIMENSIONS];
    for f in features {
        let v = f.to_vector();
        let w = f.word_count as f32 / total_words as f32;
        for (i, x) in v.iter().enumerate() {
            mean[i] += x * w;
        }
    }

    // Weighted variance → stddev
    let mut var = vec![0.0f32; FEATURE_DIMENSIONS];
    for f in features {
        let v = f.to_vector();
        let w = f.word_count as f32 / total_words as f32;
        for (i, x) in v.iter().enumerate() {
            let d = x - mean[i];
            var[i] += w * d * d;
        }
    }
    let stddev: Vec<f32> = var.iter().map(|v| v.sqrt()).collect();

    VoiceFingerprint {
        mean,
        stddev,
        passage_count: features.len() as u32,
        total_words: total_words as u32,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureDelta {
    pub label: String,
    pub fingerprint: f32,
    pub candidate: f32,
    /// (candidate - fingerprint), normalized by stddev. ±1 = one stddev off.
    pub z_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftReport {
    /// Overall L2 distance between candidate and fingerprint, normalized
    /// to [0, 1] via `tanh(d/4)` so it's a nice UI scalar.
    pub drift_score: f32,
    /// Cosine similarity (1 = identical, 0 = orthogonal). Lower = more drift.
    pub cosine: f32,
    /// Top-N largest |z_score| deltas, descending.
    pub top_deltas: Vec<FeatureDelta>,
}

pub fn compute_drift(fingerprint: &VoiceFingerprint, candidate: &str, top_n: usize) -> DriftReport {
    let cf = extract_features(candidate);
    let cv = cf.to_vector();
    let mv = &fingerprint.mean;

    // L2 distance
    let l2: f32 = mv
        .iter()
        .zip(cv.iter())
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f32>()
        .sqrt();

    // Cosine similarity
    let dot: f32 = mv.iter().zip(cv.iter()).map(|(a, b)| a * b).sum();
    let nm = mv.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nv = cv.iter().map(|x| x * x).sum::<f32>().sqrt();
    let cosine = if nm == 0.0 || nv == 0.0 {
        0.0
    } else {
        dot / (nm * nv)
    };

    // Per-feature z-score deltas
    let labels: Vec<String> = build_labels();
    let mut deltas: Vec<FeatureDelta> = labels
        .iter()
        .zip(mv.iter().zip(cv.iter().zip(fingerprint.stddev.iter())))
        .map(|(label, (m, (c, sd)))| {
            let z = if *sd > 1e-6 { (c - m) / sd } else { 0.0 };
            FeatureDelta {
                label: label.clone(),
                fingerprint: *m,
                candidate: *c,
                z_score: z,
            }
        })
        .collect();
    deltas.sort_by(|a, b| {
        b.z_score
            .abs()
            .partial_cmp(&a.z_score.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    deltas.truncate(top_n.max(1));

    let drift_score = (l2 / 4.0).tanh().clamp(0.0, 1.0);
    DriftReport {
        drift_score,
        cosine,
        top_deltas: deltas,
    }
}

fn build_labels() -> Vec<String> {
    let mut out: Vec<String> = FEATURE_LABELS.iter().map(|s| s.to_string()).collect();
    for fw in FUNCTION_WORDS {
        out.push(format!("fw \"{fw}\""));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_fingerprint_has_correct_dims() {
        let fp = VoiceFingerprint::empty();
        assert_eq!(fp.mean.len(), FEATURE_DIMENSIONS);
        assert_eq!(fp.stddev.len(), FEATURE_DIMENSIONS);
    }

    #[test]
    fn fingerprint_of_single_passage_equals_features() {
        let text = "The dragon flew over the lake. The boy watched. He waited for dawn.";
        let f = extract_features(text);
        let fp = build_fingerprint(std::slice::from_ref(&f));
        let v = f.to_vector();
        for (i, (m, expected)) in fp.mean.iter().zip(v.iter()).enumerate() {
            assert!(
                (*m - *expected).abs() < 1e-5,
                "mismatch at {i}: {m} vs {expected}",
            );
        }
    }

    #[test]
    fn fingerprint_weights_by_word_count() {
        let short = extract_features("Short. Punchy. Fast.");
        let long = extract_features(
            &"the elaborate sentence stretches itself out across the page in measured cadence "
                .repeat(20),
        );
        let fp = build_fingerprint(&[short.clone(), long.clone()]);
        // Mean sentence length (index 0) should be closer to the long
        // passage's because it has many more words.
        let sv = short.to_vector();
        let lv = long.to_vector();
        assert!(
            (fp.mean[0] - lv[0]).abs() < (fp.mean[0] - sv[0]).abs(),
            "fingerprint should weight toward long passage"
        );
    }

    #[test]
    fn drift_self_is_low() {
        let text = "The dragon flew over the lake. The boy watched. He waited for dawn.";
        let f = extract_features(text);
        let fp = build_fingerprint(&[f]);
        let drift = compute_drift(&fp, text, 5);
        assert!(drift.drift_score < 0.05);
        assert!(drift.cosine > 0.99);
    }

    #[test]
    fn drift_off_target_is_high() {
        let reference =
            "The boy waited. The dawn was slow. Cold. He looked east, then west. Quiet.";
        let candidate = "The exquisitely orchestrated mahogany corridors of the impossibly opulent ducal palace stretched before him in elaborate, near-baroque procession that defied any reasonable architectural pragmatism.";
        let fp = build_fingerprint(&[extract_features(reference)]);
        let drift = compute_drift(&fp, candidate, 5);
        assert!(drift.drift_score > 0.1, "drift={}", drift.drift_score);
        assert!(!drift.top_deltas.is_empty());
        // Mean sentence length should be one of the top deltas
        let labels: Vec<&str> = drift.top_deltas.iter().map(|d| d.label.as_str()).collect();
        assert!(labels.contains(&"mean sentence"), "labels={:?}", labels);
    }
}
