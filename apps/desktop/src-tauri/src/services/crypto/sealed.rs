//! AES-256-GCM seal/open for short blobs.

use crate::error::{QuillError, Result};
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};

const NONCE_LEN: usize = 12;
const SALT_LEN: usize = 16;

#[derive(Debug, Serialize, Deserialize)]
pub struct SealedBlob {
    pub version: u8,
    /// Argon2id salt (base64-url, no padding).
    pub salt_b64: String,
    /// AES-GCM nonce (base64-url, no padding).
    pub nonce_b64: String,
    /// Ciphertext + GCM tag (base64-url, no padding).
    pub ciphertext_b64: String,
}

pub fn seal(passphrase: &[u8], plaintext: &[u8]) -> Result<SealedBlob> {
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);

    let key_bytes = super::argon::derive_key(passphrase, &salt)?;
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| QuillError::Crypto(format!("encrypt: {e}")))?;

    Ok(SealedBlob {
        version: 1,
        salt_b64: b64e(&salt),
        nonce_b64: b64e(&nonce_bytes),
        ciphertext_b64: b64e(&ciphertext),
    })
}

pub fn open(passphrase: &[u8], blob: &SealedBlob) -> Result<Vec<u8>> {
    if blob.version != 1 {
        return Err(QuillError::Crypto(format!(
            "unknown sealed-blob version {}",
            blob.version
        )));
    }
    let salt = b64d(&blob.salt_b64)?;
    let nonce = b64d(&blob.nonce_b64)?;
    let ciphertext = b64d(&blob.ciphertext_b64)?;

    if salt.len() != SALT_LEN {
        return Err(QuillError::Crypto("invalid salt length".into()));
    }
    if nonce.len() != NONCE_LEN {
        return Err(QuillError::Crypto("invalid nonce length".into()));
    }

    let key_bytes = super::argon::derive_key(passphrase, &salt)?;
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    cipher
        .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
        .map_err(|e| QuillError::Crypto(format!("decrypt (wrong passphrase or tampered): {e}")))
}

// --- minimal base64-url, no padding (avoid an extra dep) ---

const URL_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn b64e(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    let mut i = 0;
    while i + 3 <= input.len() {
        let b = &input[i..i + 3];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
        out.push(URL_ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(URL_ALPHABET[((n >> 12) & 63) as usize] as char);
        out.push(URL_ALPHABET[((n >> 6) & 63) as usize] as char);
        out.push(URL_ALPHABET[(n & 63) as usize] as char);
        i += 3;
    }
    let rem = input.len() - i;
    if rem == 1 {
        let n = (input[i] as u32) << 16;
        out.push(URL_ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(URL_ALPHABET[((n >> 12) & 63) as usize] as char);
    } else if rem == 2 {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8);
        out.push(URL_ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(URL_ALPHABET[((n >> 12) & 63) as usize] as char);
        out.push(URL_ALPHABET[((n >> 6) & 63) as usize] as char);
    }
    out
}

fn b64d(s: &str) -> Result<Vec<u8>> {
    let mut idx = [255u8; 256];
    for (i, c) in URL_ALPHABET.iter().enumerate() {
        idx[*c as usize] = i as u8;
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity((bytes.len() / 4) * 3 + 2);
    let mut buf = [0u8; 4];
    let mut buf_len = 0;
    for &c in bytes {
        let v = idx[c as usize];
        if v == 255 {
            return Err(QuillError::Crypto(format!(
                "invalid base64-url character: {c:?}"
            )));
        }
        buf[buf_len] = v;
        buf_len += 1;
        if buf_len == 4 {
            let n = ((buf[0] as u32) << 18)
                | ((buf[1] as u32) << 12)
                | ((buf[2] as u32) << 6)
                | (buf[3] as u32);
            out.push(((n >> 16) & 0xff) as u8);
            out.push(((n >> 8) & 0xff) as u8);
            out.push((n & 0xff) as u8);
            buf_len = 0;
        }
    }
    match buf_len {
        0 => {}
        2 => {
            let n = ((buf[0] as u32) << 18) | ((buf[1] as u32) << 12);
            out.push(((n >> 16) & 0xff) as u8);
        }
        3 => {
            let n = ((buf[0] as u32) << 18) | ((buf[1] as u32) << 12) | ((buf[2] as u32) << 6);
            out.push(((n >> 16) & 0xff) as u8);
            out.push(((n >> 8) & 0xff) as u8);
        }
        _ => return Err(QuillError::Crypto("invalid base64-url length".into())),
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_and_open_roundtrip() {
        let pass = b"correct horse battery staple";
        let plaintext = b"AIzaSy_NOT_A_REAL_KEY_just_some_test_bytes_!!";
        let blob = seal(pass, plaintext).unwrap();
        let back = open(pass, &blob).unwrap();
        assert_eq!(back, plaintext);
    }

    #[test]
    fn wrong_passphrase_fails() {
        let blob = seal(b"good", b"hello").unwrap();
        assert!(open(b"bad", &blob).is_err());
    }

    #[test]
    fn nonce_changes_each_time() {
        let blob1 = seal(b"k", b"x").unwrap();
        let blob2 = seal(b"k", b"x").unwrap();
        assert_ne!(blob1.nonce_b64, blob2.nonce_b64);
        assert_ne!(blob1.salt_b64, blob2.salt_b64);
        assert_ne!(blob1.ciphertext_b64, blob2.ciphertext_b64);
    }

    #[test]
    fn b64_roundtrip() {
        for sample in [
            &b""[..],
            &b"f"[..],
            &b"fo"[..],
            &b"foo"[..],
            &b"foob"[..],
            &b"fooba"[..],
            &b"foobar"[..],
            &b"\x00\x01\x02\xff\xfe\xfd"[..],
        ] {
            let enc = b64e(sample);
            let dec = b64d(&enc).unwrap();
            assert_eq!(dec, sample);
        }
    }
}
