//! Voice fingerprint pipeline.
//!
//! What we capture per passage (deterministic, no LLM):
//!
//! Sentence rhythm:
//!   - sentence count
//!   - mean / std-dev / median sentence length (in words)
//!   - p10 / p90 sentence length (catches the "most sentences are 12 words,
//!     but the punchy ones are 4" pattern)
//!   - fragment ratio (sentences ≤4 words)
//!   - long-sentence ratio (sentences ≥30 words)
//!
//! Lexical:
//!   - type/token ratio
//!   - mean word length
//!   - dale-chall-style "easy word" ratio (top-1k frequency proxy)
//!   - function-word frequency vector (44 most common English function words)
//!
//! Dialogue:
//!   - dialogue ratio (chars inside quotes / total chars)
//!   - dialogue tag density (lines ending with "said X" patterns)
//!
//! Cadence/punctuation:
//!   - comma density (per 100 words)
//!   - em-dash density (per 100 words)
//!   - semicolon + colon density
//!   - parenthesis density
//!   - sentence-end distribution: % `.`, `!`, `?`, ellipsis, `—`
//!
//! These flatten into a single `f32` feature vector. Two passages can be
//! compared via cosine; a project's "fingerprint" is the centroid of pinned
//! references / the user's recent prose.

mod extractor;
mod fingerprint;
mod store;

#[allow(unused_imports)]
pub use extractor::{extract_features, VoiceFeatures, FEATURE_DIMENSIONS, FEATURE_LABELS};
#[allow(unused_imports)]
pub use fingerprint::{
    build_fingerprint, compute_drift, DriftReport, FeatureDelta, VoiceFingerprint,
};
pub use store::{ReferencePin, ReferencePinStore};
