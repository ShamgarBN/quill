//! Argon2id password-based key derivation.
//!
//! Parameters per OWASP Password Storage Cheat Sheet (May 2026 guidance):
//! m=64 MiB, t=3, p=1. Output: 32 bytes for AES-256.

use crate::error::{QuillError, Result};
use argon2::{Algorithm, Argon2, Params, Version};

const KEY_LEN: usize = 32; // AES-256
const M_KIB: u32 = 64 * 1024; // 64 MiB
const T_COST: u32 = 3;
const PARALLELISM: u32 = 1;

pub fn derive_key(passphrase: &[u8], salt: &[u8]) -> Result<[u8; KEY_LEN]> {
    let params = Params::new(M_KIB, T_COST, PARALLELISM, Some(KEY_LEN))
        .map_err(|e| QuillError::Crypto(format!("argon2 params: {e}")))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut out = [0u8; KEY_LEN];
    argon
        .hash_password_into(passphrase, salt, &mut out)
        .map_err(|e| QuillError::Crypto(format!("argon2 derive: {e}")))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_for_same_inputs() {
        let salt = b"sixteen-byte-slt";
        let a = derive_key(b"hunter2", salt).unwrap();
        let b = derive_key(b"hunter2", salt).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn different_passphrases_diverge() {
        let salt = b"sixteen-byte-slt";
        let a = derive_key(b"hunter2", salt).unwrap();
        let b = derive_key(b"hunter3", salt).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn different_salts_diverge() {
        let a = derive_key(b"hunter2", b"saltsaltsaltsaltA").unwrap();
        let b = derive_key(b"hunter2", b"saltsaltsaltsaltB").unwrap();
        assert_ne!(a, b);
    }
}
