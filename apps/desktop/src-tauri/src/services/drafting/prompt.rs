//! Pure prompt assembly. No IO, no state — just inputs in, ChatMessages out.
//!
//! Tested in isolation against synthetic beats / scenes / canon chunks.

use crate::models::brain::{Character, Idea, Thread, ThreadStatus};
use crate::models::canon::ChunkRef;
use crate::models::structure::{Beat, Scene};
use crate::services::drafting::orchestrator::DraftOperation;
use crate::services::llm::{ChatMessage, ChatRole, IncludedCategory};

/// Inputs to the assembler. Borrows everything so callers can build their
/// own state and pass references; nothing is cloned until the messages are
/// produced.
pub struct PromptInputs<'a> {
    pub operation: DraftOperation,
    pub instruction: &'a str,

    /// The active beat the user is writing toward. None if scene isn't
    /// linked to a beat (allowed; we just omit the macro context).
    pub beat: Option<&'a Beat>,
    pub beat_label: Option<&'a str>,
    pub beat_description: Option<&'a str>,

    pub scene: &'a Scene,
    /// The full current prose of the scene as it stands on disk. May be
    /// empty for a fresh scene.
    pub prior_text: &'a str,
    /// For `Rewrite` / `Critique`: the selected passage being acted on.
    pub selection: Option<&'a str>,

    /// Canon excerpts already filtered for `do_not_send` and ranked by
    /// retrieval relevance. The assembler does NOT re-filter — it trusts
    /// the orchestrator to have done that already.
    pub canon: &'a [ChunkRef],

    /// Setting-scoped canon excerpts retrieved by matching the scene's
    /// `setting` field against location-kind chunks. Disjoint from
    /// `canon` — the orchestrator de-duplicates by chunk id before
    /// splitting into these two slots.
    pub setting_canon: &'a [ChunkRef],

    /// The Character Bible entry that matched the scene's `pov` (by name
    /// or alias). When present, its motivation / voice / arc are injected
    /// as a dedicated context block so the model has structured info
    /// about the POV character beyond the scene's free-form POV string.
    pub pov_character: Option<&'a Character>,

    /// Idea Park entries whose tags target the active draft (by beat,
    /// scene, or POV). Already filtered to non-`do_not_send` items.
    pub ideas: &'a [Idea],

    /// Plot threads with status Open or Advancing for this project, in
    /// most-recently-updated order. The assembler marks which ones are
    /// linked to the active scene (via `linked_thread_ids`) so the model
    /// knows which threads are "live in this scene" vs "globally open."
    pub threads: &'a [Thread],

    /// Subset of `threads` (by id) that the active scene is currently
    /// tagged as touching.
    pub linked_thread_ids: &'a [String],

    /// Reference style passages (voice anchors). Each is `(label, text)`.
    /// Already filtered to enabled pins, ordered by weight, possibly
    /// truncated to a sane count by the orchestrator.
    pub voice_anchors: &'a [(String, String)],
}

/// Assembled output: the chat messages to send + the audit categories
/// describing what was included. Both must travel together so the audit
/// log accurately describes the actual call.
pub struct AssembledPrompt {
    pub messages: Vec<ChatMessage>,
    pub included: Vec<IncludedCategory>,
}

/// Assemble messages for the chat provider.
pub fn assemble_messages(inputs: &PromptInputs<'_>) -> AssembledPrompt {
    let mut included = Vec::with_capacity(8);
    included.push(IncludedCategory::SystemPrompt);

    let system = system_prompt(inputs.operation);
    let mut messages = Vec::with_capacity(2);
    messages.push(ChatMessage {
        role: ChatRole::System,
        content: system,
    });

    let user = build_user_message(inputs, &mut included);
    included.push(IncludedCategory::UserPrompt);
    messages.push(ChatMessage {
        role: ChatRole::User,
        content: user,
    });

    AssembledPrompt { messages, included }
}

fn system_prompt(op: DraftOperation) -> String {
    let core = "\
You are Quill, a writing partner for a young-adult fantasy novel.

Your discipline:
- Write tight, image-led prose that reads aloud well.
- Honor the canon excerpts provided. Never contradict them. Never invent contradictory details.
- Match the voice of the reference style passages: sentence-length variety, image grounding, dialogue with light tags.
- For YA: keep romance tender (no explicit content), violence consequential but not gratuitous.
- Track the active beat — write toward it but never name it explicitly in the prose.";

    match op {
        DraftOperation::Continue => format!(
            "{core}\n\nFor this request, you are continuing the scene from where it ends.\n\
             Output format:\n\
             - Reply with prose ONLY. No commentary, no headings, no \"Here's the continuation\".\n\
             - Stop when the active beat would land or the natural pause arrives. Don't pad."
        ),
        DraftOperation::Rewrite => format!(
            "{core}\n\nFor this request, you are rewriting the user's selected passage.\n\
             Output format:\n\
             - Reply with the rewritten prose ONLY, in the same voice. Same general scope and \
             length unless the user requests otherwise.\n\
             - Do NOT add commentary, do NOT prefix with \"Here's the rewrite\"."
        ),
        DraftOperation::Critique => format!(
            "{core}\n\nFor this request, you are critiquing the user's selected passage \
             rather than writing prose.\n\
             Output format (Markdown):\n\
             - **Voice** — does the passage match the reference style? Cite specific phrases.\n\
             - **Pacing** — does it escalate? Are there flat patches?\n\
             - **Continuity** — anything that contradicts the canon excerpts?\n\
             - **Three concrete edits** — line-level suggestions, each tied to a quoted phrase.\n\
             Be direct. Praise sparingly and only when warranted."
        ),
    }
}

fn build_user_message(inputs: &PromptInputs<'_>, included: &mut Vec<IncludedCategory>) -> String {
    let mut buf = String::with_capacity(2_048);

    // 1. Voice anchors -------------------------------------------------
    if !inputs.voice_anchors.is_empty() {
        included.push(IncludedCategory::ReferencePins);
        buf.push_str("# Reference style passages\n");
        buf.push_str(
            "Match the rhythm, image density, and dialogue cadence of these. \
             They define your voice for this novel.\n\n",
        );
        for (label, text) in inputs.voice_anchors {
            buf.push_str(&format!(
                "## {}\n\n{}\n\n",
                label,
                trim_for_prompt(text, 1_400)
            ));
        }
    }

    // 2. Canon ---------------------------------------------------------
    if !inputs.canon.is_empty() {
        included.push(IncludedCategory::CanonTopK);
        buf.push_str("# Canon excerpts (worldbuilding ground truth)\n");
        buf.push_str(
            "These are facts about your world. Stay consistent. Quote specifics where helpful.\n\n",
        );
        for c in inputs.canon {
            let crumb = if c.headings.is_empty() {
                String::new()
            } else {
                format!(" — {}", c.headings.join(" › "))
            };
            buf.push_str(&format!(
                "## {}{}\n\n{}\n\n",
                short_doc_label(c),
                crumb,
                trim_for_prompt(&c.text, 1_400),
            ));
        }
    }

    // 3. Beat context (macro) ------------------------------------------
    if let (Some(_beat), Some(label)) = (inputs.beat, inputs.beat_label) {
        included.push(IncludedCategory::BeatDescription);
        buf.push_str(&format!("# Active beat: {label}\n"));
        if let Some(desc) = inputs.beat_description {
            buf.push_str(&format!("Purpose: {desc}\n"));
        }
        let summary = inputs.beat.map(|b| b.summary.trim()).unwrap_or("");
        if !summary.is_empty() {
            buf.push_str(&format!("Your notes for this beat: {summary}\n"));
        }
        buf.push('\n');
    }

    // 3.5 POV character bio (structural) -------------------------------
    if let Some(c) = inputs.pov_character {
        included.push(IncludedCategory::CharacterBibleEntry);
        buf.push_str(&format!("# POV character: {}", c.name));
        if !c.aliases.is_empty() {
            buf.push_str(&format!(" ({})", c.aliases.join(", ")));
        }
        buf.push('\n');
        if !c.arc_one_liner.trim().is_empty() {
            buf.push_str(&format!("- Arc: {}\n", c.arc_one_liner.trim()));
        }
        if !c.motivation.trim().is_empty() {
            buf.push_str(&format!("- Motivation: {}\n", c.motivation.trim()));
        }
        if !c.voice_notes.trim().is_empty() {
            buf.push_str(&format!("- Voice: {}\n", c.voice_notes.trim()));
        }
        // `secrets` is the spoiler kill-switch. Honor `secrets_do_not_send`
        // (default true) — if the user explicitly opted them in, we can
        // include them, but the default is to redact.
        if !c.secrets.trim().is_empty() && !c.secrets_do_not_send {
            buf.push_str(&format!(
                "- Secrets (user opted in): {}\n",
                c.secrets.trim()
            ));
        }
        buf.push('\n');
    }

    // 3.55 Idea Park (capture-pile guidance) ---------------------------
    if !inputs.ideas.is_empty() {
        included.push(IncludedCategory::IdeaPark);
        buf.push_str("# Capture pile (writer's standing ideas)\n");
        buf.push_str(
            "These are notes the writer has stashed for this scene. \
             Treat as suggestions, not facts — weave in if they fit, ignore if they don't.\n\n",
        );
        for idea in inputs.ideas {
            let text = idea.text.trim();
            if text.is_empty() {
                continue;
            }
            buf.push_str("- ");
            buf.push_str(&trim_for_prompt(text, 400));
            buf.push('\n');
        }
        buf.push('\n');
    }

    // 3.57 Plot threads in motion (structural) -------------------------
    if !inputs.threads.is_empty() {
        included.push(IncludedCategory::PlotThreads);
        buf.push_str("# Plot threads in motion\n");
        buf.push_str(
            "These are open arcs the writer is tracking. Threads marked \
             [linked] are ones this scene already touches — push them \
             forward or close them. Don't introduce new contradictions.\n\n",
        );
        for t in inputs.threads {
            let linked = inputs.linked_thread_ids.iter().any(|id| id == &t.id);
            let status = match t.status {
                ThreadStatus::Open => "open",
                ThreadStatus::Advancing => "advancing",
                ThreadStatus::Resolved => "resolved",
                ThreadStatus::Abandoned => "abandoned",
            };
            buf.push_str("- ");
            if linked {
                buf.push_str("**[linked]** ");
            }
            buf.push_str(&format!("_{status}_ — **{}**", t.title.trim()));
            let desc = t.description.trim();
            if !desc.is_empty() {
                buf.push_str(": ");
                buf.push_str(&trim_for_prompt(desc, 240));
            }
            buf.push('\n');
        }
        buf.push('\n');
    }

    // 3.6 Setting-scoped canon (structural) ----------------------------
    if !inputs.setting_canon.is_empty() {
        included.push(IncludedCategory::SettingCanon);
        buf.push_str("# Setting reference\n");
        buf.push_str(
            "These describe the location the scene takes place in. Anchor sensory detail here.\n\n",
        );
        for c in inputs.setting_canon {
            let crumb = if c.headings.is_empty() {
                String::new()
            } else {
                format!(" — {}", c.headings.join(" › "))
            };
            buf.push_str(&format!(
                "## {}{}\n\n{}\n\n",
                short_doc_label(c),
                crumb,
                trim_for_prompt(&c.text, 1_400),
            ));
        }
    }

    // 4. Scene context (micro) -----------------------------------------
    included.push(IncludedCategory::SceneCard);
    buf.push_str("# Scene context\n");
    buf.push_str(&format!(
        "Title: {}\n",
        non_empty(&inputs.scene.title, "(untitled)")
    ));
    if let Some(pov) = &inputs.scene.pov {
        if !pov.trim().is_empty() {
            included.push(IncludedCategory::CharacterPov);
            buf.push_str(&format!("POV: {pov}\n"));
        }
    }
    if let Some(setting) = &inputs.scene.setting {
        if !setting.trim().is_empty() {
            buf.push_str(&format!("Setting: {setting}\n"));
        }
    }
    let mut commandments: Vec<(&str, &str)> = Vec::new();
    if !inputs.scene.inciting_incident.trim().is_empty() {
        commandments.push(("Inciting Incident", &inputs.scene.inciting_incident));
    }
    if !inputs.scene.progressive_complication.trim().is_empty() {
        commandments.push((
            "Progressive Complication",
            &inputs.scene.progressive_complication,
        ));
    }
    if !inputs.scene.crisis.trim().is_empty() {
        commandments.push(("Crisis", &inputs.scene.crisis));
    }
    if !inputs.scene.climax.trim().is_empty() {
        commandments.push(("Climax", &inputs.scene.climax));
    }
    if !inputs.scene.resolution.trim().is_empty() {
        commandments.push(("Resolution", &inputs.scene.resolution));
    }
    if !commandments.is_empty() {
        buf.push_str("Story Grid commandments:\n");
        for (label, text) in commandments {
            buf.push_str(&format!("- **{label}:** {text}\n"));
        }
    }
    buf.push('\n');

    // 5. Prior prose / selection ---------------------------------------
    match inputs.operation {
        DraftOperation::Continue => {
            if !inputs.prior_text.trim().is_empty() {
                included.push(IncludedCategory::RecentParagraphs);
                buf.push_str("# Prose so far (continue from where this ends)\n\n");
                buf.push_str(&tail_excerpt(inputs.prior_text, 1_800));
                buf.push('\n');
            } else {
                buf.push_str(
                    "# Prose so far\n_The scene is empty. Open the moment with image and grounding._\n\n",
                );
            }
        }
        DraftOperation::Rewrite | DraftOperation::Critique => {
            if let Some(sel) = inputs.selection {
                let trimmed = sel.trim();
                if !trimmed.is_empty() {
                    included.push(IncludedCategory::RecentParagraphs);
                    let header = match inputs.operation {
                        DraftOperation::Rewrite => "# Passage to rewrite",
                        DraftOperation::Critique => "# Passage to critique",
                        DraftOperation::Continue => unreachable!(),
                    };
                    buf.push_str(header);
                    buf.push_str("\n\n");
                    buf.push_str(trimmed);
                    buf.push_str("\n\n");
                }
            }
        }
    }

    // 6. Instruction ---------------------------------------------------
    buf.push_str("# Instruction\n");
    let instruction = inputs.instruction.trim();
    if instruction.is_empty() {
        buf.push_str(match inputs.operation {
            DraftOperation::Continue => {
                "Continue the scene. Push toward the active beat without naming it."
            }
            DraftOperation::Rewrite => "Rewrite the passage to better match the reference voice.",
            DraftOperation::Critique => "Critique the passage.",
        });
    } else {
        buf.push_str(instruction);
    }

    buf
}

/// Best-effort shortener: trim repeated whitespace and clip to roughly N
/// characters at a sentence boundary if possible.
fn trim_for_prompt(text: &str, max_chars: usize) -> String {
    let collapsed = collapse_ws(text);
    if collapsed.len() <= max_chars {
        return collapsed;
    }
    let cut = collapsed
        .char_indices()
        .nth(max_chars)
        .map(|(i, _)| i)
        .unwrap_or(collapsed.len());
    let head = &collapsed[..cut];
    // Try to break at the last sentence-ending punctuation we can find.
    if let Some(idx) = head.rfind(['.', '!', '?', '\n']) {
        return format!("{}…", &head[..=idx]);
    }
    format!("{head}…")
}

fn collapse_ws(text: &str) -> String {
    // Walk paragraph by paragraph (split on 2+ newlines) so paragraph breaks
    // are preserved exactly. Within a paragraph, any run of whitespace
    // (spaces, tabs, single newlines) collapses to a single space.
    let paragraphs: Vec<String> = text
        .split("\n\n")
        .map(|para| {
            let mut out = String::with_capacity(para.len());
            let mut last_was_ws = false;
            for c in para.chars() {
                if c.is_whitespace() {
                    if !last_was_ws && !out.is_empty() {
                        out.push(' ');
                        last_was_ws = true;
                    }
                } else {
                    out.push(c);
                    last_was_ws = false;
                }
            }
            out.trim().to_string()
        })
        .filter(|p| !p.is_empty())
        .collect();
    paragraphs.join("\n\n")
}

/// Return roughly the last `max_chars` of text, preferring paragraph
/// boundaries. We allow the result to overshoot up to 2× `max_chars` if
/// it lets us keep a clean break, since "the most recent paragraph"
/// matters more than hitting the budget exactly.
fn tail_excerpt(text: &str, max_chars: usize) -> String {
    let total = text.chars().count();
    if total <= max_chars {
        return text.trim().to_string();
    }
    let budget = max_chars.saturating_mul(2);

    // Paragraph-break-aware path: prefer the latest break whose suffix is
    // at most `budget` chars, falling back to smaller suffixes.
    let para_starts: Vec<usize> = text
        .match_indices("\n\n")
        .map(|(i, _)| i + "\n\n".len())
        .collect();
    for &start in &para_starts {
        let suffix_chars = text[start..].chars().count();
        if suffix_chars <= budget {
            return format!("…\n\n{}", text[start..].trim());
        }
    }

    // No paragraph break helps — hard-cut at sentence boundary.
    let skip = total - max_chars;
    let start_byte = text.char_indices().nth(skip).map(|(i, _)| i).unwrap_or(0);
    let tail = &text[start_byte..];
    if let Some(idx) = tail.find(['.', '!', '?']) {
        let mut after = idx + 1;
        while after < tail.len()
            && tail[after..]
                .chars()
                .next()
                .is_some_and(|c| c.is_whitespace())
        {
            after += tail[after..].chars().next().unwrap().len_utf8();
        }
        if after < tail.len() {
            return format!("…{}", &tail[after..]).trim().to_string();
        }
    }
    format!("…{tail}").trim().to_string()
}

fn short_doc_label(c: &ChunkRef) -> String {
    let id = &c.doc_id;
    // doc_<12hex>; show prefix for terseness.
    let short = if id.len() >= 12 {
        &id[..id.len().min(12)]
    } else {
        id.as_str()
    };
    format!("{short}#{}", c.index)
}

fn non_empty<'a>(s: &'a str, fallback: &'a str) -> &'a str {
    if s.trim().is_empty() {
        fallback
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::canon::ChunkSensitivity;
    use crate::models::structure::{BeatId, Scene};
    use chrono::Utc;

    fn fake_scene() -> Scene {
        Scene {
            id: "scn_test".to_string(),
            project_id: "p1".to_string(),
            order: 0,
            title: "The dragon's wake".into(),
            pov: Some("Kaelan".into()),
            setting: Some("Cliff above Lake Tarn".into()),
            status: crate::models::structure::SceneStatus::Drafting,
            word_count: 0,
            beat_id: Some(BeatId::Catalyst),
            inciting_incident: "A dragon dies in front of him.".into(),
            progressive_complication: "".into(),
            crisis: "".into(),
            climax: "".into(),
            resolution: "".into(),
            thread_ids: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn fake_beat() -> Beat {
        let mut b = Beat::fresh(BeatId::Catalyst);
        b.summary = "Dragon falls; Kaelan touches the scale.".into();
        b
    }

    fn fake_chunk() -> ChunkRef {
        ChunkRef {
            id: "doc_aaaa:0".into(),
            doc_id: "doc_aaaaaaaaaaaa".into(),
            project_id: "p1".into(),
            index: 0,
            offset: 0,
            text: "Lake Tarn lies west of the Hollow Wastes; its surface mirrors the storm.".into(),
            headings: vec!["Lake Tarn".into()],
            word_count: 14,
            sensitivity: ChunkSensitivity::Public,
            score: 0.9,
        }
    }

    #[test]
    fn continue_includes_prior_text_section() {
        let scene = fake_scene();
        let beat = fake_beat();
        let chunk = fake_chunk();
        let anchors = vec![(
            "Eragon ch1".to_string(),
            "He gripped the bow tighter.".to_string(),
        )];
        let inputs = PromptInputs {
            operation: DraftOperation::Continue,
            instruction: "Open with the dragon falling.",
            beat: Some(&beat),
            beat_label: Some("Catalyst"),
            beat_description: Some("Life-changing event."),
            scene: &scene,
            prior_text: "The wind tore at his hair. Kaelan shielded his eyes against the dawn.",
            selection: None,
            canon: std::slice::from_ref(&chunk),
            setting_canon: &[],
            pov_character: None,
            ideas: &[],
            threads: &[],
            linked_thread_ids: &[],
            voice_anchors: &anchors,
        };
        let assembled = assemble_messages(&inputs);
        assert_eq!(assembled.messages.len(), 2);
        let user = &assembled.messages[1].content;
        assert!(user.contains("Reference style passages"));
        assert!(user.contains("Canon excerpts"));
        assert!(user.contains("Active beat: Catalyst"));
        assert!(user.contains("Scene context"));
        assert!(user.contains("Prose so far"));
        assert!(user.contains("Open with the dragon falling"));

        // Audit categories must include the things we observe in the message.
        assert!(assembled
            .included
            .contains(&IncludedCategory::ReferencePins));
        assert!(assembled.included.contains(&IncludedCategory::CanonTopK));
        assert!(assembled
            .included
            .contains(&IncludedCategory::BeatDescription));
        assert!(assembled.included.contains(&IncludedCategory::SceneCard));
        assert!(assembled
            .included
            .contains(&IncludedCategory::RecentParagraphs));
        assert!(assembled.included.contains(&IncludedCategory::UserPrompt));
        assert!(assembled.included.contains(&IncludedCategory::CharacterPov));
    }

    #[test]
    fn rewrite_uses_selection_not_prior_text() {
        let scene = fake_scene();
        let inputs = PromptInputs {
            operation: DraftOperation::Rewrite,
            instruction: "Tighten this and amp the tension.",
            beat: None,
            beat_label: None,
            beat_description: None,
            scene: &scene,
            prior_text: "Some preceding paragraphs that should NOT appear in the prompt.",
            selection: Some("He felt a feeling of dread."),
            canon: &[],
            setting_canon: &[],
            pov_character: None,
            ideas: &[],
            threads: &[],
            linked_thread_ids: &[],
            voice_anchors: &[],
        };
        let assembled = assemble_messages(&inputs);
        let user = &assembled.messages[1].content;
        assert!(user.contains("Passage to rewrite"));
        assert!(user.contains("He felt a feeling of dread."));
        assert!(!user.contains("preceding paragraphs"));
        assert!(user.contains("Tighten this"));
    }

    #[test]
    fn critique_uses_critique_instructions() {
        let scene = fake_scene();
        let inputs = PromptInputs {
            operation: DraftOperation::Critique,
            instruction: "",
            beat: None,
            beat_label: None,
            beat_description: None,
            scene: &scene,
            prior_text: "",
            selection: Some("The wind blew. The boy was sad."),
            canon: &[],
            setting_canon: &[],
            pov_character: None,
            ideas: &[],
            threads: &[],
            linked_thread_ids: &[],
            voice_anchors: &[],
        };
        let assembled = assemble_messages(&inputs);
        let system = &assembled.messages[0].content;
        assert!(system.contains("critiquing the user's selected passage"));
        assert!(system.contains("**Voice**"));
        let user = &assembled.messages[1].content;
        assert!(user.contains("Passage to critique"));
        assert!(user.contains("The boy was sad"));
        // Default critique instruction is filled in when blank.
        assert!(user.contains("Critique the passage"));
    }

    #[test]
    fn empty_prior_text_uses_open_with_image_hint() {
        let scene = fake_scene();
        let beat = fake_beat();
        let inputs = PromptInputs {
            operation: DraftOperation::Continue,
            instruction: "Begin the scene.",
            beat: Some(&beat),
            beat_label: Some("Catalyst"),
            beat_description: None,
            scene: &scene,
            prior_text: "",
            selection: None,
            canon: &[],
            setting_canon: &[],
            pov_character: None,
            ideas: &[],
            threads: &[],
            linked_thread_ids: &[],
            voice_anchors: &[],
        };
        let user = &assemble_messages(&inputs).messages[1].content;
        assert!(user.contains("scene is empty"));
    }

    #[test]
    fn long_canon_chunks_are_truncated() {
        let mut chunk = fake_chunk();
        chunk.text = "lorem ipsum dolor sit amet. ".repeat(500);
        let scene = fake_scene();
        let inputs = PromptInputs {
            operation: DraftOperation::Continue,
            instruction: "Continue.",
            beat: None,
            beat_label: None,
            beat_description: None,
            scene: &scene,
            prior_text: "",
            selection: None,
            canon: std::slice::from_ref(&chunk),
            setting_canon: &[],
            pov_character: None,
            ideas: &[],
            threads: &[],
            linked_thread_ids: &[],
            voice_anchors: &[],
        };
        let user = &assemble_messages(&inputs).messages[1].content;
        // Should NOT contain the full repeated text. Cap is 1400 chars per
        // chunk so the message body is well under 4k.
        assert!(user.len() < 4_000, "user message was {} chars", user.len());
        assert!(user.contains("…"));
    }

    #[test]
    fn collapse_ws_preserves_paragraph_breaks() {
        let s = "alpha\n  beta\n\ngamma   delta\n\n\nepsilon";
        let collapsed = collapse_ws(s);
        // paragraphs preserved, single newlines folded to space
        assert!(collapsed.contains("alpha beta"));
        assert!(collapsed.contains("\n\ngamma delta"));
        assert!(collapsed.ends_with("epsilon"));
    }

    #[test]
    fn tail_excerpt_returns_full_text_when_short() {
        let s = "short text";
        assert_eq!(tail_excerpt(s, 100), "short text");
    }

    #[test]
    fn tail_excerpt_clips_long_text() {
        let s = "first paragraph.\n\nsecond paragraph.\n\nthird paragraph that is the most recent.";
        let tail = tail_excerpt(s, 30);
        assert!(tail.starts_with("…"));
        assert!(tail.contains("third paragraph"));
    }

    #[test]
    fn pov_character_bio_renders_when_supplied() {
        let scene = fake_scene();
        let mut character = Character::fresh("p1", "Kaelan");
        character.aliases = vec!["Kael".into()];
        character.arc_one_liner = "From runaway to reluctant leader.".into();
        character.motivation = "Avenge his father.".into();
        character.voice_notes = "Clipped, image-led, dry.".into();
        character.secrets = "Is the heir of the Hollow King.".into();
        character.secrets_do_not_send = true;

        let inputs = PromptInputs {
            operation: DraftOperation::Continue,
            instruction: "",
            beat: None,
            beat_label: None,
            beat_description: None,
            scene: &scene,
            prior_text: "",
            selection: None,
            canon: &[],
            setting_canon: &[],
            pov_character: Some(&character),
            ideas: &[],
            threads: &[],
            linked_thread_ids: &[],
            voice_anchors: &[],
        };
        let assembled = assemble_messages(&inputs);
        let user = &assembled.messages[1].content;
        assert!(user.contains("# POV character: Kaelan (Kael)"));
        assert!(user.contains("From runaway to reluctant leader"));
        assert!(user.contains("Avenge his father"));
        assert!(user.contains("Clipped, image-led, dry"));
        // Secrets must NOT leak when `secrets_do_not_send` is true.
        assert!(!user.contains("Hollow King"));
        assert!(assembled
            .included
            .contains(&IncludedCategory::CharacterBibleEntry));
    }

    #[test]
    fn pov_character_secrets_only_render_when_user_opted_in() {
        let scene = fake_scene();
        let mut character = Character::fresh("p1", "Kaelan");
        character.secrets = "Hidden bloodline.".into();
        character.secrets_do_not_send = false; // user explicitly opted in
        let inputs = PromptInputs {
            operation: DraftOperation::Continue,
            instruction: "",
            beat: None,
            beat_label: None,
            beat_description: None,
            scene: &scene,
            prior_text: "",
            selection: None,
            canon: &[],
            setting_canon: &[],
            pov_character: Some(&character),
            ideas: &[],
            threads: &[],
            linked_thread_ids: &[],
            voice_anchors: &[],
        };
        let user = &assemble_messages(&inputs).messages[1].content;
        assert!(user.contains("Hidden bloodline"));
        assert!(user.contains("user opted in"));
    }

    #[test]
    fn threads_in_motion_render_with_linked_flag() {
        use crate::models::brain::{Thread, ThreadStatus};
        let scene = fake_scene();
        let mut active = Thread::fresh("p1", "Kaelan's blood-debt to the rangers");
        active.description = "introduced ch1; must be called in by ch16".into();
        active.status = ThreadStatus::Advancing;
        let mut other = Thread::fresh("p1", "Why the bell never tolls in winter");
        other.status = ThreadStatus::Open;
        let threads = vec![active.clone(), other];
        let linked = vec![active.id.clone()];
        let inputs = PromptInputs {
            operation: DraftOperation::Continue,
            instruction: "",
            beat: None,
            beat_label: None,
            beat_description: None,
            scene: &scene,
            prior_text: "",
            selection: None,
            canon: &[],
            setting_canon: &[],
            pov_character: None,
            ideas: &[],
            threads: &threads,
            linked_thread_ids: &linked,
            voice_anchors: &[],
        };
        let assembled = assemble_messages(&inputs);
        let user = &assembled.messages[1].content;
        assert!(user.contains("# Plot threads in motion"));
        assert!(user.contains("blood-debt"));
        // Linked thread gets the [linked] marker, the other does not.
        let lines: Vec<&str> = user.lines().collect();
        let linked_line = lines.iter().find(|l| l.contains("blood-debt")).unwrap();
        let other_line = lines
            .iter()
            .find(|l| l.contains("bell never tolls"))
            .unwrap();
        assert!(linked_line.contains("[linked]"));
        assert!(!other_line.contains("[linked]"));
        assert!(assembled.included.contains(&IncludedCategory::PlotThreads));
    }

    #[test]
    fn ideas_render_as_bulleted_capture_pile() {
        let scene = fake_scene();
        let idea1 = Idea::fresh("p1", "Kaelan flinches at fire — keep the trigger small");
        let idea2 = Idea::fresh("p1", "Lake Tarn reflects no stars after the dragon falls");
        let ideas = vec![idea1, idea2];
        let inputs = PromptInputs {
            operation: DraftOperation::Continue,
            instruction: "",
            beat: None,
            beat_label: None,
            beat_description: None,
            scene: &scene,
            prior_text: "",
            selection: None,
            canon: &[],
            setting_canon: &[],
            pov_character: None,
            ideas: &ideas,
            threads: &[],
            linked_thread_ids: &[],
            voice_anchors: &[],
        };
        let assembled = assemble_messages(&inputs);
        let user = &assembled.messages[1].content;
        assert!(user.contains("# Capture pile"));
        assert!(user.contains("- Kaelan flinches at fire"));
        assert!(user.contains("- Lake Tarn reflects"));
        assert!(assembled.included.contains(&IncludedCategory::IdeaPark));
    }

    #[test]
    fn setting_canon_renders_as_its_own_section() {
        let scene = fake_scene();
        let mut chunk = fake_chunk();
        chunk.text = "Lake Tarn — cold mirror at the western edge.".into();
        let inputs = PromptInputs {
            operation: DraftOperation::Continue,
            instruction: "",
            beat: None,
            beat_label: None,
            beat_description: None,
            scene: &scene,
            prior_text: "",
            selection: None,
            canon: &[],
            setting_canon: std::slice::from_ref(&chunk),
            pov_character: None,
            ideas: &[],
            threads: &[],
            linked_thread_ids: &[],
            voice_anchors: &[],
        };
        let assembled = assemble_messages(&inputs);
        let user = &assembled.messages[1].content;
        assert!(user.contains("# Setting reference"));
        assert!(user.contains("Lake Tarn"));
        assert!(assembled.included.contains(&IncludedCategory::SettingCanon));
    }
}
