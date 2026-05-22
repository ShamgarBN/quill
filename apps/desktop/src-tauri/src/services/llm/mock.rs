//! Deterministic mock providers.
//!
//! The mock embeddings provider produces a vector by hashing the input text
//! into a fixed-dimension f32 array. Same text always produces the same
//! vector, so retrieval tests are deterministic.
//!
//! The mock chat provider echoes the last user message with a recognizable
//! prefix — handy for end-to-end UI tests.

use crate::error::Result;
use crate::services::llm::{
    ChatMessage, ChatProvider, ChatRequest, ChatResponse, ChatRole, EmbeddingsProvider,
};
use async_trait::async_trait;
use sha2::{Digest, Sha256};

pub struct MockEmbeddingsProvider {
    dims: usize,
    model: String,
}

impl MockEmbeddingsProvider {
    pub fn new(dims: usize) -> Self {
        Self {
            dims,
            model: format!("mock-{dims}d"),
        }
    }
}

#[async_trait]
impl EmbeddingsProvider for MockEmbeddingsProvider {
    fn dimensions(&self) -> usize {
        self.dims
    }

    fn model_id(&self) -> &str {
        &self.model
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| hash_to_vec(t, self.dims)).collect())
    }
}

/// Hash text into a unit-norm f32 vector of the given dimension.
///
/// We seed a SHA-256 stream and treat consecutive 4-byte windows as `i32`
/// values, mapping each to `[-1, 1]`. Then we add a TF-style term-frequency
/// signal so semantically similar strings cluster — mock embeddings need to
/// be deterministic AND minimally retrievable for tests to be meaningful.
fn hash_to_vec(text: &str, dims: usize) -> Vec<f32> {
    let mut buf = Vec::with_capacity(dims);

    // Term-frequency contribution: lowercase, ASCII-word-bag. Each word's
    // own hash modulates a few dimensions.
    let mut counts = std::collections::HashMap::<String, u32>::new();
    for w in text.split(|c: char| !c.is_alphanumeric()) {
        if w.is_empty() {
            continue;
        }
        let key = w.to_ascii_lowercase();
        *counts.entry(key).or_insert(0) += 1;
    }

    let mut tf_dims = vec![0f32; dims];
    for (w, count) in counts.iter() {
        let mut h = Sha256::new();
        h.update(w.as_bytes());
        let d = h.finalize();
        // Each word affects 4 dimensions
        for i in 0..4 {
            let idx = ((d[i * 2] as usize) << 8 | d[i * 2 + 1] as usize) % dims;
            let sign = if d[i * 2 + 2] & 1 == 0 { 1.0 } else { -1.0 };
            tf_dims[idx] += sign * (*count as f32).sqrt();
        }
    }

    // Document hash contribution: keeps documents with no shared terms apart.
    let mut h = Sha256::new();
    h.update(text.as_bytes());
    let mut doc_dims = vec![0f32; dims];
    let mut bytes = Vec::with_capacity(dims * 4);
    let mut last = h.finalize_reset();
    while bytes.len() < dims * 4 {
        bytes.extend_from_slice(&last);
        let mut h2 = Sha256::new();
        h2.update(last);
        last = h2.finalize();
    }
    for i in 0..dims {
        let chunk = [
            bytes[i * 4],
            bytes[i * 4 + 1],
            bytes[i * 4 + 2],
            bytes[i * 4 + 3],
        ];
        let raw = i32::from_le_bytes(chunk);
        doc_dims[i] = (raw as f32) / (i32::MAX as f32);
    }

    // Combine: TF dominates so similar texts cluster, hash adds spread.
    for i in 0..dims {
        buf.push(tf_dims[i] * 3.0 + doc_dims[i] * 0.2);
    }

    // L2 normalize
    let n = buf.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
    buf.iter_mut().for_each(|x| *x /= n);
    buf
}

// ---------- chat ----------

pub struct MockChatProvider {
    pub mode: MockChatMode,
}

#[derive(Debug, Clone, Copy)]
pub enum MockChatMode {
    /// Echo the last user message back verbatim, prefixed with `[mock]`.
    Echo,
    /// Always return a fixed string. Useful for narrow tests.
    Fixed(&'static str),
}

impl MockChatProvider {
    pub fn echo() -> Self {
        Self {
            mode: MockChatMode::Echo,
        }
    }
}

#[async_trait]
impl ChatProvider for MockChatProvider {
    fn provider_id(&self) -> &str {
        "mock"
    }

    fn model_id(&self) -> &str {
        "mock-chat"
    }

    async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse> {
        let last_user = req
            .messages
            .iter()
            .rev()
            .find(|m| m.role == ChatRole::User)
            .map(|m| m.content.as_str())
            .unwrap_or("");
        let content = match self.mode {
            MockChatMode::Echo => format!("[mock] {last_user}"),
            MockChatMode::Fixed(s) => s.to_string(),
        };
        let tokens_in = req
            .messages
            .iter()
            .map(|m| approx_tokens(&m.content))
            .sum::<u32>();
        let tokens_out = approx_tokens(&content);
        Ok(ChatResponse {
            content,
            tokens_in,
            tokens_out,
            model: "mock-chat".to_string(),
        })
    }
}

#[allow(dead_code)]
pub fn approx_tokens_msg(m: &ChatMessage) -> u32 {
    approx_tokens(&m.content)
}

/// Heuristic: 1 token ≈ 0.75 words for English.
pub fn approx_tokens(text: &str) -> u32 {
    let words = text.split_whitespace().count() as f32;
    (words / 0.75).round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_embeddings_are_deterministic() {
        let p = MockEmbeddingsProvider::new(64);
        let a = p.embed_batch(&["hello world"]).await.unwrap();
        let b = p.embed_batch(&["hello world"]).await.unwrap();
        assert_eq!(a, b);
    }

    #[tokio::test]
    async fn mock_embeddings_similar_texts_cluster() {
        let p = MockEmbeddingsProvider::new(128);
        let v = p
            .embed_batch(&[
                "the dragon flew over the lake",
                "the dragon soared over the lake",
            ])
            .await
            .unwrap();
        let v_diff = p
            .embed_batch(&["sandwich recipe with pickles"])
            .await
            .unwrap();

        let close = cosine(&v[0], &v[1]);
        let far = cosine(&v[0], &v_diff[0]);
        assert!(
            close > far + 0.1,
            "expected close({close}) > far({far}) + 0.1"
        );
    }

    #[test]
    fn approx_tokens_reasonable() {
        // 75 words ≈ 100 tokens
        let s = (0..75).map(|_| "word").collect::<Vec<_>>().join(" ");
        let t = approx_tokens(&s);
        assert!((90..=110).contains(&t), "got {t}");
    }

    fn cosine(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b.iter()).map(|(x, y)| x * y).sum::<f32>()
    }
}
