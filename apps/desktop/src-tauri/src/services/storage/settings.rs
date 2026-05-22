use crate::error::Result;
use crate::models::Settings;
use std::path::{Path, PathBuf};

pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("settings.json"),
        }
    }

    /// Load settings, creating defaults on first run.
    pub fn load_or_init(&self) -> Result<Settings> {
        if self.path.exists() {
            super::atomic::read_json(&self.path)
        } else {
            let s = Settings::fresh();
            super::atomic::write_json(&self.path, &s)?;
            Ok(s)
        }
    }

    pub fn save(&self, s: &Settings) -> Result<()> {
        super::atomic::write_json(&self.path, s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ThemePreference;

    #[test]
    fn fresh_install_writes_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(dir.path());
        let s = store.load_or_init().unwrap();
        assert_eq!(s.theme, ThemePreference::System);
        assert!(s.show_what_gets_sent);
        assert!(s.privacy_acknowledged_at.is_none());
        // re-load returns the same
        let again = store.load_or_init().unwrap();
        assert_eq!(again.theme, s.theme);
    }

    #[test]
    fn save_persists() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(dir.path());
        let mut s = store.load_or_init().unwrap();
        s.theme = ThemePreference::Dark;
        store.save(&s).unwrap();
        let back = store.load_or_init().unwrap();
        assert_eq!(back.theme, ThemePreference::Dark);
    }
}
