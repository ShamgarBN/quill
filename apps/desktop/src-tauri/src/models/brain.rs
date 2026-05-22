//! Phase 7 — Second-brain data models.
//!
//! Two structured stores live alongside the manuscript:
//!
//! - **Character Bible** — one entry per character, with motivation,
//!   voice, secrets, and arc one-liner. Each entry is editable in place
//!   and cross-references back to canon chunks + scenes that mention the
//!   character.
//! - **Idea Park** — a flat list of timestamped, tagged scratch notes.
//!   Capture-fast, organize-later.
//!
//! Both are stored as JSON files under the project root:
//!
//!   <project>/bible/characters.json
//!   <project>/bible/ideas.json
//!
//! Privacy discipline:
//! - Each character has a `secrets` field that may carry plot reveals or
//!   DM-only material. The `secrets_do_not_send` flag, when true, keeps
//!   that field out of any prompt assembled by the drafting orchestrator.
//! - Ideas are flagged the same way via `do_not_send` per-idea.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Optional structured role label. Free-form `String` would also work,
/// but a small enum nudges the user toward useful taxonomy.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum CharacterRole {
    Protagonist,
    Antagonist,
    Mentor,
    Ally,
    LoveInterest,
    Family,
    Foil,
    #[default]
    Supporting,
    Minor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character {
    pub id: String,
    pub project_id: String,
    pub name: String,
    /// Alternate names, nicknames, titles. Used for cross-link matching.
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub role: CharacterRole,
    #[serde(default)]
    pub motivation: String,
    #[serde(default)]
    pub voice_notes: String,
    #[serde(default)]
    pub secrets: String,
    /// When true, `secrets` is treated as `do_not_send` for any prompt
    /// assembly (drafting / critique).
    #[serde(default = "default_true")]
    pub secrets_do_not_send: bool,
    #[serde(default)]
    pub arc_one_liner: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn default_true() -> bool {
    true
}

impl Character {
    pub fn fresh(project_id: &str, name: &str) -> Self {
        let now = Utc::now();
        Self {
            id: format!("char_{}", Uuid::new_v4().simple()),
            project_id: project_id.to_string(),
            name: name.to_string(),
            aliases: Vec::new(),
            role: CharacterRole::Supporting,
            motivation: String::new(),
            voice_notes: String::new(),
            secrets: String::new(),
            secrets_do_not_send: true,
            arc_one_liner: String::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Iterate over the names this character matches against — primary
    /// name plus any aliases.
    pub fn match_terms(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.name.as_str()).chain(self.aliases.iter().map(|s| s.as_str()))
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CharacterPatch {
    pub name: Option<String>,
    pub aliases: Option<Vec<String>>,
    pub role: Option<CharacterRole>,
    pub motivation: Option<String>,
    pub voice_notes: Option<String>,
    pub secrets: Option<String>,
    pub secrets_do_not_send: Option<bool>,
    pub arc_one_liner: Option<String>,
}

impl CharacterPatch {
    pub fn apply(self, c: &mut Character) {
        if let Some(v) = self.name {
            c.name = v;
        }
        if let Some(v) = self.aliases {
            c.aliases = v;
        }
        if let Some(v) = self.role {
            c.role = v;
        }
        if let Some(v) = self.motivation {
            c.motivation = v;
        }
        if let Some(v) = self.voice_notes {
            c.voice_notes = v;
        }
        if let Some(v) = self.secrets {
            c.secrets = v;
        }
        if let Some(v) = self.secrets_do_not_send {
            c.secrets_do_not_send = v;
        }
        if let Some(v) = self.arc_one_liner {
            c.arc_one_liner = v;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Idea {
    pub id: String,
    pub project_id: String,
    pub text: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// True = never reveal to a cloud LLM. Useful for plot twists you
    /// don't want spoiled by retrieval.
    #[serde(default)]
    pub do_not_send: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Idea {
    pub fn fresh(project_id: &str, text: &str) -> Self {
        let now = Utc::now();
        Self {
            id: format!("idea_{}", Uuid::new_v4().simple()),
            project_id: project_id.to_string(),
            text: text.to_string(),
            tags: Vec::new(),
            do_not_send: false,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct IdeaPatch {
    pub text: Option<String>,
    pub tags: Option<Vec<String>>,
    pub do_not_send: Option<bool>,
}

impl IdeaPatch {
    pub fn apply(self, i: &mut Idea) {
        if let Some(v) = self.text {
            i.text = v;
        }
        if let Some(v) = self.tags {
            i.tags = v;
        }
        if let Some(v) = self.do_not_send {
            i.do_not_send = v;
        }
    }
}

/// A single match returned by character cross-link queries. Generic over
/// the source kind so the UI can render all matches in one list.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum CrossLink {
    /// A scene whose metadata or body text mentions the character.
    #[serde(rename = "scene")]
    Scene {
        scene_id: String,
        order: u32,
        title: String,
        /// Specific term in the character's `match_terms` that hit.
        matched_term: String,
        /// Where the match landed: "title", "pov", "setting", "summary",
        /// or "body".
        location: String,
        /// Short snippet around the match (body location only).
        snippet: Option<String>,
    },
    /// A canon chunk whose text mentions the character.
    #[serde(rename = "canon")]
    Canon {
        chunk_id: String,
        doc_id: String,
        matched_term: String,
        snippet: String,
        headings: Vec<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn character_match_terms_includes_name_and_aliases() {
        let mut c = Character::fresh("p1", "Kaelan");
        c.aliases = vec!["Kael".into(), "the boy of Tarn".into()];
        let terms: Vec<&str> = c.match_terms().collect();
        assert_eq!(terms, vec!["Kaelan", "Kael", "the boy of Tarn"]);
    }

    #[test]
    fn character_patch_only_changes_set_fields() {
        let mut c = Character::fresh("p1", "Kaelan");
        c.role = CharacterRole::Mentor;
        c.motivation = "x".into();
        let patch = CharacterPatch {
            motivation: Some("revenge for his father".into()),
            ..CharacterPatch::default()
        };
        patch.apply(&mut c);
        assert_eq!(c.motivation, "revenge for his father");
        // Role untouched
        assert_eq!(c.role, CharacterRole::Mentor);
    }

    #[test]
    fn idea_default_does_not_set_do_not_send() {
        let i = Idea::fresh("p1", "Tarn freezes after midnight");
        assert!(!i.do_not_send);
    }
}
