//! Secret store: a single sealed JSON file holding a map of named secrets.
//!
//! Phase 0 uses a fixed app-derived passphrase mixed with a per-install salt.
//! This is NOT a substitute for a user-chosen passphrase or macOS Keychain
//! (we'll add Keychain integration in Phase 2 alongside actual API keys).
//!
//! For Phase 0, the goal is to prove the encryption pipeline end-to-end and
//! lock the on-disk format so future phases can layer Keychain on top
//! without changing the file shape.

use crate::error::{QuillError, Result};
use rand::{rngs::OsRng, RngCore};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const SECRETS_FILENAME: &str = "secrets.enc";
const INSTALL_KEY_FILENAME: &str = ".install-key";

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct SecretsMap {
    items: BTreeMap<String, super::sealed::SealedBlob>,
}

pub struct SecretStore {
    file: PathBuf,
    install_key: Vec<u8>,
    inner: Mutex<SecretsMap>,
}

impl SecretStore {
    /// Initialize the store, creating the install-key file on first run.
    pub fn initialize(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir)?;

        let install_key_path = data_dir.join(INSTALL_KEY_FILENAME);
        let install_key = if install_key_path.exists() {
            std::fs::read(&install_key_path)?
        } else {
            let mut k = [0u8; 32];
            OsRng.fill_bytes(&mut k);
            // 0600 perms via OpenOptions on Unix
            #[cfg(unix)]
            {
                use std::io::Write;
                use std::os::unix::fs::OpenOptionsExt;
                let mut f = std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .mode(0o600)
                    .open(&install_key_path)?;
                f.write_all(&k)?;
                f.sync_all()?;
            }
            #[cfg(not(unix))]
            {
                std::fs::write(&install_key_path, &k)?;
            }
            k.to_vec()
        };

        let file = data_dir.join(SECRETS_FILENAME);
        let map: SecretsMap = if file.exists() {
            super::super::storage::atomic_read_json_or_default(&file)?
        } else {
            SecretsMap::default()
        };

        Ok(Self {
            file,
            install_key,
            inner: Mutex::new(map),
        })
    }

    pub fn set(&self, key: &str, value: &str) -> Result<()> {
        if key.is_empty() {
            return Err(QuillError::InvalidArgument("empty secret key".into()));
        }
        let blob = super::sealed::seal(&self.install_key, value.as_bytes())?;
        let mut g = self.inner.lock().expect("secret store mutex poisoned");
        g.items.insert(key.to_string(), blob);
        super::super::storage::atomic_write_json(&self.file, &*g)?;
        Ok(())
    }

    pub fn get(&self, key: &str) -> Result<Option<String>> {
        let g = self.inner.lock().expect("secret store mutex poisoned");
        let Some(blob) = g.items.get(key) else {
            return Ok(None);
        };
        let bytes = super::sealed::open(&self.install_key, blob)?;
        Ok(Some(String::from_utf8(bytes)?))
    }

    pub fn has(&self, key: &str) -> bool {
        let g = self.inner.lock().expect("secret store mutex poisoned");
        g.items.contains_key(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_persist_across_reload() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = SecretStore::initialize(dir.path()).unwrap();
            store.set("gemini", "AIzaSy_FAKE_KEY_FOR_TEST").unwrap();
            assert!(store.has("gemini"));
            assert_eq!(
                store.get("gemini").unwrap().as_deref(),
                Some("AIzaSy_FAKE_KEY_FOR_TEST")
            );
        }
        {
            let store = SecretStore::initialize(dir.path()).unwrap();
            assert_eq!(
                store.get("gemini").unwrap().as_deref(),
                Some("AIzaSy_FAKE_KEY_FOR_TEST")
            );
            assert_eq!(store.get("missing").unwrap(), None);
        }
    }

    #[test]
    fn rejects_empty_key() {
        let dir = tempfile::tempdir().unwrap();
        let store = SecretStore::initialize(dir.path()).unwrap();
        assert!(matches!(
            store.set("", "x"),
            Err(QuillError::InvalidArgument(_))
        ));
    }
}
