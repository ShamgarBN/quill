//! Encryption-at-rest for sensitive blobs (API keys, etc.).
//!
//! - KDF: Argon2id, m=64 MiB, t=3, p=1 (OWASP modern guidance, May 2026)
//! - Cipher: AES-256-GCM with random 12-byte nonces
//! - Salt: random 16 bytes, stored alongside the ciphertext
//! - Format on disk: bincode-free JSON for portability and human inspection
//!
//! Bulk content (manuscript, canon) intentionally is NOT encrypted by Quill;
//! macOS FileVault provides at-rest protection and the user can open any
//! Markdown file in any editor at any time. See docs/PRIVACY.md.

mod argon;
mod sealed;
mod store;

pub use store::SecretStore;
