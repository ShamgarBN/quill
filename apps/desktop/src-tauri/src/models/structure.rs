//! Structural engine data model.
//!
//! Two layered systems, both first-class:
//! - **Save the Cat (macro)** — a 15-beat sheet anchoring the novel.
//! - **Story Grid (micro)** — five commandments per scene (Inciting Incident,
//!   Progressive Complication, Crisis, Climax, Resolution).
//!
//! Both are *fluid by default, lockable on demand* — the user can pin a beat
//! to a specific manuscript position, mark a beat satisfied, or freeze the
//! whole sheet against further drift.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// One of the canonical 15 Save-the-Cat beats. Order is fixed; the slug is
/// the stable identifier.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum BeatId {
    OpeningImage,
    ThemeStated,
    SetUp,
    Catalyst,
    Debate,
    BreakIntoTwo,
    BStory,
    FunAndGames,
    Midpoint,
    BadGuysCloseIn,
    AllIsLost,
    DarkNightOfTheSoul,
    BreakIntoThree,
    Finale,
    FinalImage,
}

impl BeatId {
    pub const ALL: [BeatId; 15] = [
        BeatId::OpeningImage,
        BeatId::ThemeStated,
        BeatId::SetUp,
        BeatId::Catalyst,
        BeatId::Debate,
        BeatId::BreakIntoTwo,
        BeatId::BStory,
        BeatId::FunAndGames,
        BeatId::Midpoint,
        BeatId::BadGuysCloseIn,
        BeatId::AllIsLost,
        BeatId::DarkNightOfTheSoul,
        BeatId::BreakIntoThree,
        BeatId::Finale,
        BeatId::FinalImage,
    ];

    pub fn order(self) -> u8 {
        Self::ALL.iter().position(|&b| b == self).unwrap_or(0) as u8
    }

    pub fn label(self) -> &'static str {
        match self {
            BeatId::OpeningImage => "Opening Image",
            BeatId::ThemeStated => "Theme Stated",
            BeatId::SetUp => "Set-Up",
            BeatId::Catalyst => "Catalyst",
            BeatId::Debate => "Debate",
            BeatId::BreakIntoTwo => "Break Into Two",
            BeatId::BStory => "B Story",
            BeatId::FunAndGames => "Fun and Games",
            BeatId::Midpoint => "Midpoint",
            BeatId::BadGuysCloseIn => "Bad Guys Close In",
            BeatId::AllIsLost => "All Is Lost",
            BeatId::DarkNightOfTheSoul => "Dark Night of the Soul",
            BeatId::BreakIntoThree => "Break Into Three",
            BeatId::Finale => "Finale",
            BeatId::FinalImage => "Final Image",
        }
    }

    /// Stable kebab-case slug matching the serde serialized form. Used as
    /// the canonical id in tag conventions (e.g. `beat:catalyst`) and as
    /// the URL-safe key whenever a string is needed.
    pub fn as_slug(self) -> &'static str {
        match self {
            BeatId::OpeningImage => "opening-image",
            BeatId::ThemeStated => "theme-stated",
            BeatId::SetUp => "set-up",
            BeatId::Catalyst => "catalyst",
            BeatId::Debate => "debate",
            BeatId::BreakIntoTwo => "break-into-two",
            BeatId::BStory => "b-story",
            BeatId::FunAndGames => "fun-and-games",
            BeatId::Midpoint => "midpoint",
            BeatId::BadGuysCloseIn => "bad-guys-close-in",
            BeatId::AllIsLost => "all-is-lost",
            BeatId::DarkNightOfTheSoul => "dark-night-of-the-soul",
            BeatId::BreakIntoThree => "break-into-three",
            BeatId::Finale => "finale",
            BeatId::FinalImage => "final-image",
        }
    }

    /// Target position as a fraction of total manuscript length.
    pub fn target_pct(self) -> f32 {
        match self {
            BeatId::OpeningImage => 0.01,
            BeatId::ThemeStated => 0.05,
            BeatId::SetUp => 0.05,
            BeatId::Catalyst => 0.10,
            BeatId::Debate => 0.15,
            BeatId::BreakIntoTwo => 0.20,
            BeatId::BStory => 0.22,
            BeatId::FunAndGames => 0.35,
            BeatId::Midpoint => 0.50,
            BeatId::BadGuysCloseIn => 0.62,
            BeatId::AllIsLost => 0.75,
            BeatId::DarkNightOfTheSoul => 0.80,
            BeatId::BreakIntoThree => 0.85,
            BeatId::Finale => 0.92,
            BeatId::FinalImage => 0.99,
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            BeatId::OpeningImage => {
                "First impression of the world and tone — opposite of Final Image."
            }
            BeatId::ThemeStated => "A side character voices the story's question or thesis.",
            BeatId::SetUp => "Hero's flawed status quo: home, work, play, what needs fixing.",
            BeatId::Catalyst => "Life-changing event that disrupts the status quo.",
            BeatId::Debate => "Hero hesitates, weighs the cost, asks 'should I?'",
            BeatId::BreakIntoTwo => "Hero commits and steps into the new world.",
            BeatId::BStory => "Romance/mentor subplot begins; vehicle for the theme.",
            BeatId::FunAndGames => "Premise on display — the trailer moments.",
            BeatId::Midpoint => "False victory or false defeat; stakes raised.",
            BeatId::BadGuysCloseIn => "Internal flaws + external pressure mount.",
            BeatId::AllIsLost => "Lowest external moment — a 'whiff of death.'",
            BeatId::DarkNightOfTheSoul => "Internal collapse; hero confronts the lie.",
            BeatId::BreakIntoThree => "New plan synthesizing A and B story lessons.",
            BeatId::Finale => "Hero executes the plan, transforms, defeats antagonist.",
            BeatId::FinalImage => "Mirror of Opening Image — proof of change.",
        }
    }
}

/// Where a beat sits in the manuscript and how the user has tagged it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Beat {
    pub id: BeatId,
    /// User-authored notes/intent for this beat.
    pub summary: String,
    /// Optional override of the canonical target percentage. None means use
    /// `BeatId::target_pct()` * `target_word_count`.
    pub override_pct: Option<f32>,
    /// Approximate word offset where the beat is satisfied. None until the
    /// user pins it.
    pub anchor_word: Option<u32>,
    /// User has confirmed the beat is satisfied.
    pub satisfied: bool,
    /// User has frozen the beat — generation must respect it.
    pub locked: bool,
}

impl Beat {
    pub fn fresh(id: BeatId) -> Self {
        Self {
            id,
            summary: String::new(),
            override_pct: None,
            anchor_word: None,
            satisfied: false,
            locked: false,
        }
    }

    pub fn target_word(&self, total_target: u32) -> u32 {
        let pct = self.override_pct.unwrap_or_else(|| self.id.target_pct());
        (pct * total_target as f32).round() as u32
    }
}

/// Full beat sheet for a project. One per project; stored at
/// `<project>/structure/beat_sheet.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeatSheet {
    pub project_id: String,
    pub target_word_count: u32,
    pub beats: Vec<Beat>,
    pub frozen: bool,
    pub updated_at: DateTime<Utc>,
}

impl BeatSheet {
    /// Default for YA fantasy — 90k target words, all beats unanchored.
    pub fn fresh(project_id: &str) -> Self {
        Self {
            project_id: project_id.to_string(),
            target_word_count: 90_000,
            beats: BeatId::ALL.iter().map(|&id| Beat::fresh(id)).collect(),
            frozen: false,
            updated_at: Utc::now(),
        }
    }
}

// --------- Scene (Story Grid micro-structure) ---------

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum SceneStatus {
    #[default]
    Outlined,
    Drafting,
    Drafted,
    Revised,
    Locked,
}

/// A single scene within the manuscript. Owns the Story Grid five
/// commandments inline so the user can fill them progressively.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Scene {
    pub id: String,
    pub project_id: String,
    /// Index in the manuscript (sequential, dense). The user can reorder
    /// with explicit move ops.
    pub order: u32,
    pub title: String,
    /// POV character name. Free text — no Character Bible coupling yet.
    pub pov: Option<String>,
    pub setting: Option<String>,
    pub status: SceneStatus,
    pub word_count: u32,
    pub beat_id: Option<BeatId>,
    /// Story Grid five commandments (all optional / fillable).
    pub inciting_incident: String,
    pub progressive_complication: String,
    pub crisis: String,
    pub climax: String,
    pub resolution: String,
    /// Plot threads this scene touches (introduces, advances, or resolves).
    /// IDs reference `Thread.id` from the project's thread store.
    /// Defaulted in serde for backward compat with v0.2 scene records.
    pub thread_ids: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Default for Scene {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            id: String::new(),
            project_id: String::new(),
            order: 0,
            title: String::new(),
            pov: None,
            setting: None,
            status: SceneStatus::Outlined,
            word_count: 0,
            beat_id: None,
            inciting_incident: String::new(),
            progressive_complication: String::new(),
            crisis: String::new(),
            climax: String::new(),
            resolution: String::new(),
            thread_ids: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }
}

impl Scene {
    pub fn fresh(project_id: &str, order: u32, title: &str) -> Self {
        let now = Utc::now();
        Self {
            id: format!("scn_{}", uuid::Uuid::new_v4().simple()),
            project_id: project_id.to_string(),
            order,
            title: title.to_string(),
            pov: None,
            setting: None,
            status: SceneStatus::Outlined,
            word_count: 0,
            beat_id: None,
            inciting_incident: String::new(),
            progressive_complication: String::new(),
            crisis: String::new(),
            climax: String::new(),
            resolution: String::new(),
            thread_ids: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SceneList {
    pub scenes: Vec<Scene>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_beat_sheet_has_15_beats_in_order() {
        let bs = BeatSheet::fresh("p1");
        assert_eq!(bs.beats.len(), 15);
        for (i, b) in bs.beats.iter().enumerate() {
            assert_eq!(b.id.order() as usize, i);
        }
    }

    #[test]
    fn target_word_uses_override_when_present() {
        let mut b = Beat::fresh(BeatId::Midpoint);
        assert_eq!(b.target_word(100_000), 50_000);
        b.override_pct = Some(0.45);
        assert_eq!(b.target_word(100_000), 45_000);
    }

    #[test]
    fn beat_descriptions_are_distinct() {
        let mut seen = std::collections::HashSet::new();
        for &id in &BeatId::ALL {
            assert!(
                seen.insert(id.description()),
                "duplicate description for {id:?}"
            );
        }
    }
}
