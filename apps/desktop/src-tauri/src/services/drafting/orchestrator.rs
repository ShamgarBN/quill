//! Drafting orchestrator: side-effectful glue between the structure store,
//! manuscript content, canon retrieval, voice anchors, drift gate, the LLM
//! provider, and the audit log.

use crate::error::{QuillError, Result};
use crate::models::brain::{Character, Idea, Thread, WorldEntry};
use crate::models::canon::{CanonKind, ChunkRef};
use crate::models::structure::{Beat, Scene};
use crate::services::brain::{CharacterStore, IdeaStore, ThreadStore, WorldStore};
use crate::services::canon::IngestService;
use crate::services::drafting::prompt::{assemble_messages, PromptInputs};
use crate::services::llm::{
    AuditEntry, AuditLog, ChatMessage, ChatProvider, ChatRequest, EmbeddingsProvider,
    IncludedCategory, ProviderRegistry,
};
use crate::services::manuscript::ManuscriptStore;
use crate::services::storage::ProjectStore;
use crate::services::structure::StructureStore;
use crate::services::vector::VectorStore;
use crate::services::voice::{compute_drift, ReferencePinStore};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// What the user is asking the model to do.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DraftOperation {
    /// Continue from the end of the scene.
    Continue,
    /// Rewrite a selected passage.
    Rewrite,
    /// Critique a selected passage; the model returns notes, not prose.
    Critique,
}

impl DraftOperation {
    pub fn audit_label(self) -> &'static str {
        match self {
            DraftOperation::Continue => "draft_continuation",
            DraftOperation::Rewrite => "rewrite_passage",
            DraftOperation::Critique => "critique_passage",
        }
    }
}

/// Request body for both preview and send.
#[derive(Debug, Clone, Deserialize)]
pub struct DraftRequest {
    pub project_id: String,
    pub scene_id: String,
    pub operation: DraftOperation,
    #[serde(default)]
    pub instruction: String,
    #[serde(default)]
    pub selection: Option<String>,
    /// How many canon excerpts to retrieve. Capped at 10.
    #[serde(default)]
    pub top_k_canon: Option<u32>,
    /// How many voice anchors to include (top-N by weight). Capped at 5.
    #[serde(default)]
    pub max_voice_anchors: Option<u32>,
    /// User explicitly chose to draft despite the drift gate firing.
    #[serde(default)]
    pub override_drift_gate: bool,
}

/// Drift gate threshold — if the current scene's voice drift exceeds this,
/// the orchestrator refuses `Continue` / `Rewrite` calls unless the user
/// overrides. Tuned to match the manuscript editor's "high drift" red zone.
pub const DRIFT_GATE_THRESHOLD: f32 = 0.70;

/// Don't even compute drift below this many words — the score is unreliable.
pub const DRIFT_MIN_WORDS: u32 = 30;

/// Description of the call as it would be sent. Built by `preview` and
/// the first phase of `invoke` so the UI can show a "what gets sent" panel.
#[derive(Debug, Clone, Serialize)]
pub struct DraftPreview {
    pub messages: Vec<ChatMessage>,
    pub included: Vec<IncludedCategory>,
    pub canon_chunk_count: u32,
    pub voice_anchor_count: u32,
    /// `None` if no fingerprint exists or the scene is below `DRIFT_MIN_WORDS`.
    pub current_drift: Option<f32>,
    /// True if `current_drift >= DRIFT_GATE_THRESHOLD`.
    pub drift_blocks_send: bool,
    pub canon_chunks: Vec<ChunkRef>,
    /// Name of the Character Bible entry matched by the scene's POV, if any.
    /// `None` when the scene has no POV set or no matching character exists.
    pub pov_character_name: Option<String>,
    /// Number of setting-scoped canon chunks injected as a separate block.
    pub setting_canon_count: u32,
    /// Number of Idea Park entries auto-matched by tag and included.
    pub idea_count: u32,
    /// Number of plot threads (open + advancing) included in the prompt.
    pub thread_count: u32,
    /// Number of those threads that the active scene is currently linked to.
    pub linked_thread_count: u32,
    /// Number of curated World Bible entries matched into the prompt.
    pub world_entry_count: u32,
    pub provider: String,
    pub model: String,
}

/// Result of an actual LLM call.
#[derive(Debug, Clone, Serialize)]
pub struct DraftSuggestion {
    pub content: String,
    pub provider: String,
    pub model: String,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub current_drift: Option<f32>,
    pub canon_chunks_used: Vec<ChunkRef>,
    pub override_drift_gate: bool,
}

/// Public service handle. Holds Arc references to the shared subsystems and
/// borrows the project store for the duration of a call.
pub struct DraftingService<'a> {
    pub projects: &'a ProjectStore,
    pub vectors: Arc<dyn VectorStore>,
    pub embedder: Arc<dyn EmbeddingsProvider>,
    pub providers: &'a ProviderRegistry,
    pub audit: Arc<AuditLog>,
}

impl<'a> DraftingService<'a> {
    /// Build a preview without making the LLM call. Always-on; useful for
    /// the "what gets sent" panel and CI-style schema checks.
    pub async fn preview(
        &self,
        chat_provider: Arc<dyn ChatProvider>,
        req: &DraftRequest,
    ) -> Result<DraftPreview> {
        let DraftContext {
            scene,
            beat,
            beat_label,
            beat_description,
            prior_text,
            canon_chunks,
            setting_canon,
            pov_character,
            ideas,
            threads,
            linked_thread_ids,
            world_entries,
            voice_anchors,
            current_drift,
        } = self.gather_context(req).await?;

        let drift_blocks_send = current_drift
            .map(|d| {
                matches!(
                    req.operation,
                    DraftOperation::Continue | DraftOperation::Rewrite
                ) && d >= DRIFT_GATE_THRESHOLD
            })
            .unwrap_or(false);

        let inputs = PromptInputs {
            operation: req.operation,
            instruction: &req.instruction,
            beat: beat.as_ref(),
            beat_label: beat_label.as_deref(),
            beat_description: beat_description.as_deref(),
            scene: &scene,
            prior_text: &prior_text,
            selection: req.selection.as_deref(),
            canon: &canon_chunks,
            setting_canon: &setting_canon,
            pov_character: pov_character.as_ref(),
            ideas: &ideas,
            threads: &threads,
            linked_thread_ids: &linked_thread_ids,
            world_entries: &world_entries,
            voice_anchors: &voice_anchors,
        };
        let assembled = assemble_messages(&inputs);

        Ok(DraftPreview {
            messages: assembled.messages,
            included: assembled.included,
            canon_chunk_count: canon_chunks.len() as u32,
            voice_anchor_count: voice_anchors.len() as u32,
            current_drift,
            drift_blocks_send,
            canon_chunks,
            pov_character_name: pov_character.as_ref().map(|c| c.name.clone()),
            setting_canon_count: setting_canon.len() as u32,
            idea_count: ideas.len() as u32,
            thread_count: threads.len() as u32,
            linked_thread_count: linked_thread_ids.len() as u32,
            world_entry_count: world_entries.len() as u32,
            provider: chat_provider.provider_id().to_string(),
            model: chat_provider.model_id().to_string(),
        })
    }

    /// Run the full pipeline: gather context, build the prompt, enforce the
    /// drift gate (unless overridden), invoke the chat provider, audit the
    /// call. Returns the suggestion or a `QuillError`.
    pub async fn invoke(
        &self,
        chat_provider: Arc<dyn ChatProvider>,
        req: &DraftRequest,
    ) -> Result<DraftSuggestion> {
        let DraftContext {
            scene,
            beat,
            beat_label,
            beat_description,
            prior_text,
            canon_chunks,
            setting_canon,
            pov_character,
            ideas,
            threads,
            linked_thread_ids,
            world_entries,
            voice_anchors,
            current_drift,
        } = self.gather_context(req).await?;

        // Drift gate. Critique is always allowed; the gate is only there to
        // stop the user from extending already-off-voice prose.
        let drift_blocks = current_drift
            .map(|d| {
                matches!(
                    req.operation,
                    DraftOperation::Continue | DraftOperation::Rewrite
                ) && d >= DRIFT_GATE_THRESHOLD
            })
            .unwrap_or(false);
        if drift_blocks && !req.override_drift_gate {
            return Err(QuillError::InvalidArgument(format!(
                "drift gate: current scene voice drift is {:.2} (\u{2265} {:.2}). \
                 Pause and re-anchor your voice, or pass override_drift_gate=true to proceed.",
                current_drift.unwrap_or(0.0),
                DRIFT_GATE_THRESHOLD,
            )));
        }

        let inputs = PromptInputs {
            operation: req.operation,
            instruction: &req.instruction,
            beat: beat.as_ref(),
            beat_label: beat_label.as_deref(),
            beat_description: beat_description.as_deref(),
            scene: &scene,
            prior_text: &prior_text,
            selection: req.selection.as_deref(),
            canon: &canon_chunks,
            setting_canon: &setting_canon,
            pov_character: pov_character.as_ref(),
            ideas: &ideas,
            threads: &threads,
            linked_thread_ids: &linked_thread_ids,
            world_entries: &world_entries,
            voice_anchors: &voice_anchors,
        };
        let assembled = assemble_messages(&inputs);

        let request = ChatRequest::new(assembled.messages.clone());
        let started = Utc::now();
        let result = chat_provider.chat(&request).await;
        let (success, error) = match &result {
            Ok(_) => (true, None),
            Err(err) => (false, Some(err.to_string())),
        };
        let entry = AuditEntry {
            timestamp: started,
            provider: chat_provider.provider_id().to_string(),
            model: chat_provider.model_id().to_string(),
            operation: req.operation.audit_label().to_string(),
            project_id: Some(req.project_id.clone()),
            scene_id: Some(req.scene_id.clone()),
            tokens_in: result.as_ref().map(|r| r.tokens_in).unwrap_or(0),
            tokens_out: result.as_ref().map(|r| r.tokens_out).unwrap_or(0),
            included: assembled.included,
            success,
            error,
        };
        if let Err(e) = self.audit.append(&entry) {
            tracing::warn!(error = %crate::error::DisplayErr(&e),
                           "audit append failed; suggestion still returned");
        }

        let response = result?;
        Ok(DraftSuggestion {
            content: response.content,
            provider: chat_provider.provider_id().to_string(),
            model: response.model,
            tokens_in: response.tokens_in,
            tokens_out: response.tokens_out,
            current_drift,
            canon_chunks_used: canon_chunks,
            override_drift_gate: req.override_drift_gate,
        })
    }

    async fn gather_context(&self, req: &DraftRequest) -> Result<DraftContext> {
        let structure = StructureStore::new(self.projects);
        let scenes = structure.load_scenes(&req.project_id)?;
        let scene = scenes
            .iter()
            .find(|s| s.id == req.scene_id)
            .cloned()
            .ok_or_else(|| QuillError::NotFound(format!("scene {}", req.scene_id)))?;

        let beat_sheet = structure.load_beat_sheet(&req.project_id)?;
        let (beat, beat_label, beat_description) = match scene.beat_id {
            Some(id) => {
                let b = beat_sheet.beats.iter().find(|b| b.id == id).cloned();
                (
                    b,
                    Some(id.label().to_string()),
                    Some(id.description().to_string()),
                )
            }
            None => (None, None, None),
        };

        let manuscript = ManuscriptStore::new(self.projects);
        let content = manuscript.load_scene(&req.project_id, &scene.id, scene.order)?;
        let prior_text = content.text.clone();

        // Canon retrieval. The query is the scene's beat-summary + commandments
        // + instruction so retrieval reflects what we're about to write.
        let mut query_parts: Vec<String> = Vec::new();
        if let Some(b) = beat.as_ref() {
            if !b.summary.trim().is_empty() {
                query_parts.push(b.summary.clone());
            }
        }
        if let Some(label) = beat_label.as_ref() {
            query_parts.push(label.clone());
        }
        if let Some(setting) = scene.setting.as_ref() {
            query_parts.push(setting.clone());
        }
        if let Some(pov) = scene.pov.as_ref() {
            query_parts.push(pov.clone());
        }
        if !req.instruction.trim().is_empty() {
            query_parts.push(req.instruction.clone());
        }
        if let Some(sel) = req.selection.as_ref() {
            // Only the first 200 chars to keep the embedding budget tight.
            let s: String = sel.chars().take(200).collect();
            query_parts.push(s);
        }
        let canon_chunks = if query_parts.is_empty() {
            Vec::new()
        } else {
            let query = query_parts.join(" ");
            let k = req.top_k_canon.unwrap_or(5).clamp(0, 10) as usize;
            if k == 0 {
                Vec::new()
            } else {
                let ingest = IngestService::new(self.embedder.as_ref(), self.vectors.as_ref());
                // Always respect do-not-send during drafting; the user can't
                // override that here.
                ingest.retrieve(&req.project_id, &query, k, true).await?
            }
        };

        // Voice anchors: top-N enabled pins by weight, content-trimmed.
        let pins_store = ReferencePinStore::new(self.projects);
        let mut pins = pins_store.list(&req.project_id)?;
        pins.retain(|p| p.enabled && !p.passage.trim().is_empty());
        pins.sort_by(|a, b| {
            b.weight
                .partial_cmp(&a.weight)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let max_anchors = req.max_voice_anchors.unwrap_or(3).clamp(0, 5) as usize;
        pins.truncate(max_anchors);
        let voice_anchors: Vec<(String, String)> =
            pins.into_iter().map(|p| (p.label, p.passage)).collect();

        // Voice drift on existing scene text — only meaningful with a
        // fingerprint AND enough material. The fingerprint helper handles
        // the empty case by returning a zero fingerprint, so we filter on
        // passage_count > 0.
        let fingerprint = pins_store.fingerprint(&req.project_id)?;
        let current_drift =
            if fingerprint.passage_count > 0 && content.word_count >= DRIFT_MIN_WORDS {
                Some(compute_drift(&fingerprint, &prior_text, 1).drift_score)
            } else {
                None
            };

        // POV character lookup: match the scene's `pov` string against
        // every character's name + aliases (case-insensitive). The match
        // succeeds when ANY term is a substring of the POV string, so a
        // scene POV of "Kaelan, 3rd-limited" still matches a character
        // named "Kaelan". First match wins; order matches CharacterStore::list.
        let pov_character: Option<Character> = if let Some(pov_raw) = scene
            .pov
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let pov_lower = pov_raw.to_lowercase();
            let characters = CharacterStore::new(self.projects).list(&req.project_id)?;
            characters.into_iter().find(|c| {
                c.match_terms()
                    .any(|t| !t.trim().is_empty() && pov_lower.contains(&t.to_lowercase()))
            })
        } else {
            None
        };

        // Setting-scoped canon: when the scene declares a setting, embed
        // that string and pull the top-2 location / cosmology chunks that
        // match. De-duplicate against `canon_chunks` (the main semantic
        // pool) so we don't double-budget the same text.
        let setting_canon: Vec<ChunkRef> = if let Some(setting) = scene
            .setting
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let embeds = self.embedder.embed_batch(&[setting]).await?;
            if let Some(q_vec) = embeds.into_iter().next() {
                let raw = self
                    .vectors
                    .search_by_kind(
                        &req.project_id,
                        &q_vec,
                        &[CanonKind::Location, CanonKind::Cosmology],
                        2,
                        true,
                    )
                    .await?;
                let already: std::collections::HashSet<&str> =
                    canon_chunks.iter().map(|c| c.id.as_str()).collect();
                raw.into_iter()
                    .filter(|c| !already.contains(c.id.as_str()))
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // Idea Park: pull up to 5 ideas whose tags target the active
        // beat / scene / POV. The store is the source of truth for the
        // `do_not_send` filter.
        let beat_tag = scene.beat_id.map(|b| b.as_slug().to_string());
        let pov_name = scene
            .pov
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from);
        let ideas = IdeaStore::new(self.projects).relevant_for_draft(
            &req.project_id,
            beat_tag.as_deref(),
            Some(scene.id.as_str()),
            pov_name.as_deref(),
            5,
        )?;

        // Plot threads: all open/advancing threads for the project, plus
        // the subset the active scene is currently linked to.
        let threads = ThreadStore::new(self.projects).in_motion(&req.project_id)?;
        // Only keep linked ids that actually exist in the active thread set,
        // so a stale id on a scene doesn't claim membership in nothing.
        let in_motion_ids: std::collections::HashSet<&str> =
            threads.iter().map(|t| t.id.as_str()).collect();
        let linked_thread_ids: Vec<String> = scene
            .thread_ids
            .iter()
            .filter(|id| in_motion_ids.contains(id.as_str()))
            .cloned()
            .collect();

        // World Bible: curated places / factions / lore whose name or
        // alias is mentioned by the scene's setting, the instruction, or
        // the scene's recent prose. These are the user-maintained
        // authoritative entries — the prompt tells the model they outrank
        // raw canon excerpts.
        let world_entries = match_world_entries(
            WorldStore::new(self.projects).list(&req.project_id)?,
            scene.setting.as_deref(),
            &req.instruction,
            &prior_text,
            MAX_WORLD_ENTRIES,
        );

        Ok(DraftContext {
            scene,
            beat,
            beat_label,
            beat_description,
            prior_text,
            canon_chunks,
            setting_canon,
            pov_character,
            ideas,
            threads,
            linked_thread_ids,
            world_entries,
            voice_anchors,
            current_drift,
        })
    }
}

/// Cap on World Bible entries injected per draft. Six curated paragraphs
/// is plenty of grounding without crowding out the prose context.
const MAX_WORLD_ENTRIES: usize = 6;

/// How much of the scene's trailing prose to scan for entry mentions.
const WORLD_MATCH_PROSE_TAIL_CHARS: usize = 1_500;

/// Select the World Bible entries relevant to this draft, prioritized:
/// setting matches (3) > instruction matches (2) > recent-prose matches (1).
/// Names/aliases shorter than 3 chars are ignored to avoid noise hits.
fn match_world_entries(
    entries: Vec<WorldEntry>,
    setting: Option<&str>,
    instruction: &str,
    prior_text: &str,
    cap: usize,
) -> Vec<WorldEntry> {
    if entries.is_empty() || cap == 0 {
        return Vec::new();
    }
    let setting_l = setting.unwrap_or("").to_lowercase();
    let instruction_l = instruction.to_lowercase();
    // Tail of the prose, sliced on a char boundary.
    let tail_start = prior_text
        .char_indices()
        .rev()
        .nth(WORLD_MATCH_PROSE_TAIL_CHARS.saturating_sub(1))
        .map(|(i, _)| i)
        .unwrap_or(0);
    let prose_l = prior_text[tail_start..].to_lowercase();

    let mut scored: Vec<(u8, WorldEntry)> = Vec::new();
    for w in entries {
        let mut terms: Vec<String> = vec![w.name.trim().to_lowercase()];
        terms.extend(w.aliases.iter().map(|a| a.trim().to_lowercase()));
        terms.retain(|t| t.len() >= 3);
        if terms.is_empty() {
            continue;
        }
        let hit = |hay: &str| terms.iter().any(|t| hay.contains(t.as_str()));
        let score = if hit(&setting_l) {
            3
        } else if hit(&instruction_l) {
            2
        } else if hit(&prose_l) {
            1
        } else {
            0
        };
        if score > 0 {
            scored.push((score, w));
        }
    }
    // Stable sort keeps the store's (alphabetical) order within a tier.
    scored.sort_by_key(|s| std::cmp::Reverse(s.0));
    scored.truncate(cap);
    scored.into_iter().map(|(_, w)| w).collect()
}

/// Internal struct collecting everything `gather_context` produces.
struct DraftContext {
    scene: Scene,
    beat: Option<Beat>,
    beat_label: Option<String>,
    beat_description: Option<String>,
    prior_text: String,
    canon_chunks: Vec<ChunkRef>,
    setting_canon: Vec<ChunkRef>,
    pov_character: Option<Character>,
    ideas: Vec<Idea>,
    threads: Vec<Thread>,
    linked_thread_ids: Vec<String>,
    world_entries: Vec<WorldEntry>,
    voice_anchors: Vec<(String, String)>,
    current_drift: Option<f32>,
}

// `BeatId::label` and `BeatId::description` are already defined on the
// model type in `models/structure.rs`; we just call them from the
// orchestrator's `gather_context`.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::canon::ChunkSensitivity;
    use crate::models::structure::BeatId;
    use crate::services::llm::{
        AuditLog, MockChatProvider, MockEmbeddingsProvider, ProviderRegistry,
    };
    use crate::services::manuscript::ManuscriptStore;
    use crate::services::storage::ProjectStore;
    use crate::services::structure::StructureStore;
    use crate::services::vector::JsonVectorStore;
    use crate::services::voice::ReferencePinStore;

    use crate::models::brain::WorldKind;

    fn world(name: &str, kind: WorldKind, aliases: &[&str]) -> WorldEntry {
        let mut w = WorldEntry::fresh("p1", name, kind);
        w.aliases = aliases.iter().map(|s| s.to_string()).collect();
        w
    }

    #[test]
    fn world_match_prioritizes_setting_then_instruction_then_prose() {
        let entries = vec![
            world("Cinterra", WorldKind::Location, &[]),
            world("Coven of Shadows", WorldKind::Faction, &["the Coven"]),
            world("Meridian Grid", WorldKind::Lore, &[]),
            world("Volara", WorldKind::Location, &[]),
        ];
        let picked = match_world_entries(
            entries,
            Some("the Scrape, beneath Cinterra"),
            "have the coven agents close in",
            "…the hum of the Meridian Grid overhead never stopped.",
            6,
        );
        let names: Vec<&str> = picked.iter().map(|w| w.name.as_str()).collect();
        // Setting hit first, instruction hit (via alias) second, prose hit third.
        assert_eq!(names, vec!["Cinterra", "Coven of Shadows", "Meridian Grid"]);
    }

    #[test]
    fn world_match_caps_results_and_ignores_short_terms() {
        let mut entries: Vec<WorldEntry> = (0..10)
            .map(|i| world(&format!("Place{i}"), WorldKind::Location, &[]))
            .collect();
        // Two-char name must never match, even though "ar" appears everywhere.
        entries.push(world("ar", WorldKind::Lore, &[]));
        let picked = match_world_entries(
            entries,
            Some("Place0 Place1 Place2 Place3 Place4 Place5 Place6 ar"),
            "",
            "",
            4,
        );
        assert_eq!(picked.len(), 4, "capped at 4");
        assert!(picked.iter().all(|w| w.name != "ar"), "short names ignored");
    }

    #[test]
    fn world_match_scans_only_the_prose_tail() {
        let entries = vec![world("Thalvenor", WorldKind::Location, &[])];
        // Mention sits ~10k chars before the end — outside the scanned tail.
        let mut prose = String::from("Thalvenor was far behind them now. ");
        prose.push_str(&"x".repeat(10_000));
        let picked = match_world_entries(entries, None, "", &prose, 6);
        assert!(
            picked.is_empty(),
            "early-scene mentions beyond the tail window don't match"
        );
    }

    struct Harness {
        _tmp: tempfile::TempDir,
        projects: ProjectStore,
        vectors: Arc<dyn VectorStore>,
        embedder: Arc<dyn EmbeddingsProvider>,
        audit: Arc<AuditLog>,
        providers: ProviderRegistry,
        project_id: String,
        scene_id: String,
    }

    fn harness() -> Harness {
        let tmp = tempfile::tempdir().unwrap();
        let projects = ProjectStore::new(tmp.path());
        let project = projects.create("Demo").unwrap();
        let scenes = StructureStore::new(&projects);
        let mut sheet = scenes.load_beat_sheet(&project.id).unwrap();
        for b in sheet.beats.iter_mut() {
            if b.id == BeatId::Catalyst {
                b.summary = "Dragon falls; Kaelan touches the scale.".into();
            }
        }
        scenes.save_beat_sheet(&sheet).unwrap();
        let scene = scenes
            .create_scene(&project.id, "Dragon's wake", Some(BeatId::Catalyst))
            .unwrap();

        let vectors: Arc<dyn VectorStore> =
            Arc::new(JsonVectorStore::open(tmp.path().join("vectors.json")).unwrap());
        let embedder: Arc<dyn EmbeddingsProvider> = Arc::new(MockEmbeddingsProvider::new(64));
        let audit = Arc::new(AuditLog::open(tmp.path()).unwrap());
        // Provider registry is only used by callers that need real chat
        // providers — our tests build their own MockChatProvider, so we
        // pass through a registry whose secrets are empty.
        let secrets =
            Arc::new(crate::services::crypto::SecretStore::initialize(tmp.path()).unwrap());
        let providers = ProviderRegistry::new(secrets);

        Harness {
            project_id: project.id.clone(),
            scene_id: scene.id,
            _tmp: tmp,
            projects,
            vectors,
            embedder,
            audit,
            providers,
        }
    }

    fn pin_voice_anchors(h: &Harness) {
        let pins = ReferencePinStore::new(&h.projects);
        pins.create(
            &h.project_id,
            "Eragon ch1",
            "He gripped the bow tighter. The wind cut. The boy held his ground.",
        )
        .unwrap();
    }

    async fn ingest_canon(h: &Harness) {
        // Plant a canon chunk via the underlying VectorStore so we don't
        // depend on a writable tmp file for the canon ingest pipeline.
        use crate::models::canon::CanonChunk;
        let chunk = CanonChunk {
            id: "doc_test:0".into(),
            doc_id: "doc_test".into(),
            project_id: h.project_id.clone(),
            index: 0,
            offset: 0,
            text: "Lake Tarn lies west of the Hollow Wastes; its surface mirrors the storm.".into(),
            headings: vec!["Lake Tarn".into()],
            word_count: 14,
            sensitivity: ChunkSensitivity::Public,
            source_path: String::new(),
            kind: CanonKind::Lore,
        };
        // Mock embedder is deterministic — embedding the chunk text gives a
        // vector that the same query will retrieve.
        let vec = h
            .embedder
            .embed_batch(&[chunk.text.as_str()])
            .await
            .unwrap()
            .pop()
            .unwrap();
        h.vectors.insert_many(&[(chunk, vec)]).await.unwrap();
    }

    fn build_service(h: &Harness) -> DraftingService<'_> {
        DraftingService {
            projects: &h.projects,
            vectors: h.vectors.clone(),
            embedder: h.embedder.clone(),
            providers: &h.providers,
            audit: h.audit.clone(),
        }
    }

    #[tokio::test]
    async fn preview_returns_messages_and_audit_categories() {
        let h = harness();
        pin_voice_anchors(&h);
        ingest_canon(&h).await;

        let svc = build_service(&h);
        let chat: Arc<dyn ChatProvider> = Arc::new(MockChatProvider::echo());
        let req = DraftRequest {
            project_id: h.project_id.clone(),
            scene_id: h.scene_id.clone(),
            operation: DraftOperation::Continue,
            instruction: "Open the moment with the dragon's body falling.".into(),
            selection: None,
            top_k_canon: Some(3),
            max_voice_anchors: Some(2),
            override_drift_gate: false,
        };
        let preview = svc.preview(chat, &req).await.unwrap();
        assert_eq!(preview.messages.len(), 2);
        assert!(preview.included.contains(&IncludedCategory::ReferencePins));
        assert!(preview
            .included
            .contains(&IncludedCategory::BeatDescription));
        assert!(preview.canon_chunk_count >= 1);
        assert_eq!(preview.voice_anchor_count, 1);
        assert!(!preview.drift_blocks_send);
        assert_eq!(preview.provider, "mock");
    }

    #[tokio::test]
    async fn invoke_runs_provider_and_appends_audit_entry() {
        let h = harness();
        let svc = build_service(&h);
        let chat: Arc<dyn ChatProvider> = Arc::new(MockChatProvider::echo());
        let req = DraftRequest {
            project_id: h.project_id.clone(),
            scene_id: h.scene_id.clone(),
            operation: DraftOperation::Continue,
            instruction: "Begin.".into(),
            selection: None,
            top_k_canon: Some(0),
            max_voice_anchors: Some(0),
            override_drift_gate: false,
        };
        let suggestion = svc.invoke(chat, &req).await.unwrap();
        assert!(suggestion.content.starts_with("[mock]"));
        let entries = h.audit.tail(10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].operation, "draft_continuation");
        assert_eq!(
            entries[0].project_id.as_deref(),
            Some(h.project_id.as_str())
        );
        assert_eq!(entries[0].scene_id.as_deref(), Some(h.scene_id.as_str()));
        assert!(entries[0].success);
    }

    #[tokio::test]
    async fn drift_gate_blocks_continue_when_voice_too_far_off() {
        let h = harness();
        pin_voice_anchors(&h);

        // Write a wildly off-voice scene so the drift score crosses the gate.
        let manuscript = ManuscriptStore::new(&h.projects);
        let off_voice = "The exquisitely orchestrated mahogany corridors of the impossibly opulent ducal palace stretched before him in elaborate, near-baroque procession that defied any reasonable architectural pragmatism, embellished with serpentine flourishes that wound themselves about the fluted columns in the manner of an heirloom carpet, lavish in its embellishments and unhurried in its progress.".repeat(2);
        let structure = StructureStore::new(&h.projects);
        let scene = structure
            .load_scenes(&h.project_id)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        manuscript
            .save_scene(&h.project_id, &scene.id, scene.order, &off_voice)
            .unwrap();

        let svc = build_service(&h);
        let chat: Arc<dyn ChatProvider> = Arc::new(MockChatProvider::echo());
        let req = DraftRequest {
            project_id: h.project_id.clone(),
            scene_id: h.scene_id.clone(),
            operation: DraftOperation::Continue,
            instruction: "Continue.".into(),
            selection: None,
            top_k_canon: Some(0),
            max_voice_anchors: Some(0),
            override_drift_gate: false,
        };
        let err = svc.invoke(chat.clone(), &req).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("drift gate"), "msg={msg}");

        // With override, it goes through.
        let req2 = DraftRequest {
            override_drift_gate: true,
            ..req
        };
        let suggestion = svc.invoke(chat, &req2).await.unwrap();
        assert!(suggestion.override_drift_gate);
    }

    #[tokio::test]
    async fn critique_bypasses_drift_gate() {
        let h = harness();
        pin_voice_anchors(&h);
        let manuscript = ManuscriptStore::new(&h.projects);
        let structure = StructureStore::new(&h.projects);
        let scene = structure
            .load_scenes(&h.project_id)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let off_voice = "The exquisitely orchestrated mahogany corridors of the impossibly opulent ducal palace stretched before him in elaborate, near-baroque procession that defied any reasonable architectural pragmatism.".repeat(3);
        manuscript
            .save_scene(&h.project_id, &scene.id, scene.order, &off_voice)
            .unwrap();

        let svc = build_service(&h);
        let chat: Arc<dyn ChatProvider> = Arc::new(MockChatProvider::echo());
        let req = DraftRequest {
            project_id: h.project_id.clone(),
            scene_id: h.scene_id.clone(),
            operation: DraftOperation::Critique,
            instruction: "Critique.".into(),
            selection: Some(off_voice.chars().take(200).collect()),
            top_k_canon: Some(0),
            max_voice_anchors: Some(0),
            override_drift_gate: false,
        };
        // Should succeed even without override, since critique is allowed
        // through the gate.
        let _ = svc.invoke(chat, &req).await.unwrap();
    }
}
