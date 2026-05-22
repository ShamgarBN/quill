use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    Light,
    Dark,
    #[default]
    System,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum FontPreference {
    #[default]
    Charter,
    JetbrainsMono,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum GenerationMode {
    Scene,
    #[default]
    Paragraph,
    Sentence,
}

/// User-facing settings. Persisted as plain JSON in `<data>/settings.json`.
///
/// Sensitive values (API keys etc.) DO NOT live here — they go through
/// `services::crypto::SecretStore` and are sealed at rest.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    pub theme: ThemePreference,
    pub prose_font: FontPreference,
    pub privacy_acknowledged_at: Option<DateTime<Utc>>,
    pub default_generation_mode: GenerationMode,
    pub show_what_gets_sent: bool,
}

impl Settings {
    /// Default settings for a fresh install. We default `show_what_gets_sent`
    /// to true so the user sees exactly what's transmitted on their first
    /// cloud call — they can disable it later in Settings.
    pub fn fresh() -> Self {
        Self {
            theme: ThemePreference::System,
            prose_font: FontPreference::Charter,
            privacy_acknowledged_at: None,
            default_generation_mode: GenerationMode::Paragraph,
            show_what_gets_sent: true,
        }
    }
}

/// Patch type used for partial updates. All fields optional.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct SettingsPatch {
    pub theme: Option<ThemePreference>,
    pub prose_font: Option<FontPreference>,
    pub privacy_acknowledged_at: Option<Option<DateTime<Utc>>>,
    pub default_generation_mode: Option<GenerationMode>,
    pub show_what_gets_sent: Option<bool>,
}

impl SettingsPatch {
    pub fn apply(self, s: &mut Settings) {
        if let Some(v) = self.theme {
            s.theme = v;
        }
        if let Some(v) = self.prose_font {
            s.prose_font = v;
        }
        if let Some(v) = self.privacy_acknowledged_at {
            s.privacy_acknowledged_at = v;
        }
        if let Some(v) = self.default_generation_mode {
            s.default_generation_mode = v;
        }
        if let Some(v) = self.show_what_gets_sent {
            s.show_what_gets_sent = v;
        }
    }
}
