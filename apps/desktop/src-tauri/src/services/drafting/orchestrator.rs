//! Drafting orchestrator: side-effectful glue between the structure store,
//! manuscript content, canon retrieval, voice anchors, drift gate, the LLM
//! provider, and the audit log.

use crate::error::{QuillError, Result};
use crate::models::canon::ChunkRef;
use crate::models::structure::{Beat, Scene};
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

        Ok(DraftContext {
            scene,
            beat,
            beat_label,
            beat_description,
            prior_text,
            canon_chunks,
            voice_anchors,
            current_drift,
        })
    }
}

/// Internal struct collecting everything `gather_context` produces.
struct DraftContext {
    scene: Scene,
    beat: Option<Beat>,
    beat_label: Option<String>,
    beat_description: Option<String>,
    prior_text: String,
    canon_chunks: Vec<ChunkRef>,
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
