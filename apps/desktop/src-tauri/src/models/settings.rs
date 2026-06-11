use crate::services::llm::ProviderId;
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

/// Target readership band. "8-18" is two different markets: middle grade
/// and YA differ in vocabulary, sentence complexity, content lines, and
/// theme depth. The drafting/critique prompts calibrate to this, and the
/// editor's readability indicator scores against it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum AgeBand {
    /// Ages 8-12.
    MiddleGrade,
    /// Ages 13-18.
    #[default]
    YoungAdult,
}

/// User-facing settings. Persisted as plain JSON in `<data>/settings.json`.
///
/// Sensitive values (API keys etc.) DO NOT live here — they go through
/// `services::crypto::SecretStore` and are sealed at rest.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemePreference,
    pub prose_font: FontPreference,
    pub privacy_acknowledged_at: Option<DateTime<Utc>>,
    pub default_generation_mode: GenerationMode,
    pub show_what_gets_sent: bool,
    /// Active chat provider for drafting + critique. Default Mock so a
    /// fresh install never makes a network call before the user opts in.
    pub chat_provider: ProviderId,
    /// Active embeddings provider. Default Mock; user opts into Gemini.
    pub embedding_provider: ProviderId,
    /// Target readership band for drafting, critique, and readability.
    pub target_age_band: AgeBand,
}

impl Default for Settings {
    fn default() -> Self {
        Self::fresh()
    }
}

impl Settings {
    /// Default settings for a fresh install. We default `show_what_gets_sent`
    /// to true so the user sees exactly what's transmitted on their first
    /// cloud call. Provider defaults are Mock so the app never reaches the
    /// network until the user explicitly switches.
    pub fn fresh() -> Self {
        Self {
            theme: ThemePreference::System,
            prose_font: FontPreference::Charter,
            privacy_acknowledged_at: None,
            default_generation_mode: GenerationMode::Paragraph,
            show_what_gets_sent: true,
            chat_provider: ProviderId::Mock,
            embedding_provider: ProviderId::Mock,
            target_age_band: AgeBand::YoungAdult,
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
    pub chat_provider: Option<ProviderId>,
    pub embedding_provider: Option<ProviderId>,
    pub target_age_band: Option<AgeBand>,
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
        if let Some(v) = self.chat_provider {
            s.chat_provider = v;
        }
        if let Some(v) = self.embedding_provider {
            s.embedding_provider = v;
        }
        if let Some(v) = self.target_age_band {
            s.target_age_band = v;
        }
    }
}
