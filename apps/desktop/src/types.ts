/**
 * Shared TypeScript types that mirror the Rust serde-serialized models.
 *
 * Keep this file in lockstep with `apps/desktop/src-tauri/src/models/` and
 * `apps/desktop/src-tauri/src/services/llm/`. Field names use snake_case to
 * match Rust default serde output.
 */

export type ThemePreference = "light" | "dark" | "system";

export type FontPreference = "charter" | "jetbrains-mono";

export type GenerationMode = "scene" | "paragraph" | "sentence";

export type ProviderId = "gemini" | "groq" | "mock";

export type CanonKind =
  | "character"
  | "location"
  | "faction"
  | "magic"
  | "history"
  | "cosmology"
  | "timeline"
  | "lore"
  | "plot_notes"
  | "dm_notes"
  | "other";

export type ChunkSensitivity = "public" | "spoiler" | "do_not_send";

export type SourceKind = "markdown" | "plain" | "pdf";

export interface Project {
  id: string;
  name: string;
  created_at: string;
  updated_at: string;
  manuscript_word_count: number;
  beat_progress: number;
  vault_path: string | null;
  vault_auto_watch: boolean;
}

export interface ProjectPatch {
  name?: string;
  /** `null` to clear, `string` to set. Omit to leave unchanged. */
  vault_path?: string | null;
  vault_auto_watch?: boolean;
}

export interface WatchStatus {
  project_id: string;
  vault_path: string;
  started_at: string;
  events_received: number;
  files_reingested: number;
  last_event_at: string | null;
  last_event_path: string | null;
  last_error: string | null;
}

export interface Settings {
  theme: ThemePreference;
  prose_font: FontPreference;
  privacy_acknowledged_at: string | null;
  default_generation_mode: GenerationMode;
  show_what_gets_sent: boolean;
  chat_provider: ProviderId;
  embedding_provider: ProviderId;
}

export interface SettingsPatch {
  theme?: ThemePreference;
  prose_font?: FontPreference;
  privacy_acknowledged_at?: string | null;
  default_generation_mode?: GenerationMode;
  show_what_gets_sent?: boolean;
  chat_provider?: ProviderId;
  embedding_provider?: ProviderId;
}

export interface CommitInfo {
  oid: string;
  short_oid: string;
  message: string;
  timestamp: string;
  files_changed: number;
}

export interface CanonDocument {
  id: string;
  project_id: string;
  source_path: string;
  kind: CanonKind;
  source_kind: SourceKind;
  ingested_at: string;
  updated_at: string;
  chunk_count: number;
  byte_size: number;
}

export interface IngestReport {
  document: CanonDocument;
  chunks_emitted: number;
  bytes_read: number;
}

export interface ChunkRef {
  id: string;
  doc_id: string;
  project_id: string;
  index: number;
  offset: number;
  text: string;
  headings: string[];
  word_count: number;
  sensitivity: ChunkSensitivity;
  score: number;
}

export type IncludedCategory =
  | "scene_card"
  | "beat_description"
  | "character_pov"
  | "recent_paragraphs"
  | "canon_top_k"
  | "reference_pins"
  | "user_prompt"
  | "system_prompt";

export interface AuditEntry {
  timestamp: string;
  provider: string;
  model: string;
  operation: string;
  project_id: string | null;
  scene_id: string | null;
  tokens_in: number;
  tokens_out: number;
  included: IncludedCategory[];
  success: boolean;
  error: string | null;
}

export interface ProviderStatus {
  provider: ProviderId;
  has_key: boolean;
}

// ---------- Structure (Phase 3) ----------

export type BeatId =
  | "opening-image"
  | "theme-stated"
  | "set-up"
  | "catalyst"
  | "debate"
  | "break-into-two"
  | "b-story"
  | "fun-and-games"
  | "midpoint"
  | "bad-guys-close-in"
  | "all-is-lost"
  | "dark-night-of-the-soul"
  | "break-into-three"
  | "finale"
  | "final-image";

export type SceneStatus = "outlined" | "drafting" | "drafted" | "revised" | "locked";

export interface Beat {
  id: BeatId;
  summary: string;
  override_pct: number | null;
  anchor_word: number | null;
  satisfied: boolean;
  locked: boolean;
}

export interface BeatSheet {
  project_id: string;
  target_word_count: number;
  beats: Beat[];
  frozen: boolean;
  updated_at: string;
}

export interface BeatPatch {
  summary?: string;
  override_pct?: number | null;
  anchor_word?: number | null;
  satisfied?: boolean;
  locked?: boolean;
}

export interface Scene {
  id: string;
  project_id: string;
  order: number;
  title: string;
  pov: string | null;
  setting: string | null;
  status: SceneStatus;
  word_count: number;
  beat_id: BeatId | null;
  inciting_incident: string;
  progressive_complication: string;
  crisis: string;
  climax: string;
  resolution: string;
  created_at: string;
  updated_at: string;
}

export interface ScenePatch {
  title?: string;
  pov?: string | null;
  setting?: string | null;
  status?: SceneStatus;
  word_count?: number;
  beat_id?: BeatId | null;
  inciting_incident?: string;
  progressive_complication?: string;
  crisis?: string;
  climax?: string;
  resolution?: string;
}

export interface ImportedBeat {
  id: BeatId;
  label: string;
  summary: string;
}

export interface ImportPreview {
  matched: ImportedBeat[];
  unmatched: string[];
}

// ---------- Manuscript content (Phase 5) ----------

export interface SceneContent {
  scene_id: string;
  /** Absolute path to the on-disk Markdown file. */
  path: string;
  text: string;
  word_count: number;
  char_count: number;
}

// ---------- Voice (Phase 4) ----------

export interface ReferencePin {
  id: string;
  project_id: string;
  label: string;
  author: string | null;
  source: string | null;
  passage: string;
  weight: number;
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface ReferencePinPatch {
  label?: string;
  author?: string | null;
  source?: string | null;
  passage?: string;
  weight?: number;
  enabled?: boolean;
}

export interface VoiceFeatures {
  sentence_count: number;
  word_count: number;
  char_count: number;
  mean_sentence_words: number;
  stddev_sentence_words: number;
  median_sentence_words: number;
  p10_sentence_words: number;
  p90_sentence_words: number;
  fragment_ratio: number;
  long_sentence_ratio: number;
  type_token_ratio: number;
  mean_word_length: number;
  dialogue_ratio: number;
  dialogue_tag_density: number;
  comma_density: number;
  emdash_density: number;
  semicolon_density: number;
  colon_density: number;
  paren_density: number;
  period_pct: number;
  bang_pct: number;
  question_pct: number;
  ellipsis_pct: number;
  emdash_end_pct: number;
  function_word_freq: number[];
}

export interface VoiceFingerprint {
  mean: number[];
  stddev: number[];
  passage_count: number;
  total_words: number;
}

export interface FeatureDelta {
  label: string;
  fingerprint: number;
  candidate: number;
  z_score: number;
}

export interface DriftReport {
  drift_score: number;
  cosine: number;
  top_deltas: FeatureDelta[];
}

export interface QuillError {
  kind: string;
  message: string;
}

// ---------- Second brain (Phase 7) ----------

export type CharacterRole =
  | "protagonist"
  | "antagonist"
  | "mentor"
  | "ally"
  | "love-interest"
  | "family"
  | "foil"
  | "supporting"
  | "minor";

export interface Character {
  id: string;
  project_id: string;
  name: string;
  aliases: string[];
  role: CharacterRole;
  motivation: string;
  voice_notes: string;
  secrets: string;
  secrets_do_not_send: boolean;
  arc_one_liner: string;
  created_at: string;
  updated_at: string;
}

export interface CharacterPatch {
  name?: string;
  aliases?: string[];
  role?: CharacterRole;
  motivation?: string;
  voice_notes?: string;
  secrets?: string;
  secrets_do_not_send?: boolean;
  arc_one_liner?: string;
}

export interface Idea {
  id: string;
  project_id: string;
  text: string;
  tags: string[];
  do_not_send: boolean;
  created_at: string;
  updated_at: string;
}

export interface IdeaPatch {
  text?: string;
  tags?: string[];
  do_not_send?: boolean;
}

export type CrossLink =
  | {
      kind: "scene";
      scene_id: string;
      order: number;
      title: string;
      matched_term: string;
      location: string;
      snippet: string | null;
    }
  | {
      kind: "canon";
      chunk_id: string;
      doc_id: string;
      matched_term: string;
      snippet: string;
      headings: string[];
    };

export const CHARACTER_ROLE_LABELS: Record<CharacterRole, string> = {
  protagonist: "Protagonist",
  antagonist: "Antagonist",
  mentor: "Mentor",
  ally: "Ally",
  "love-interest": "Love Interest",
  family: "Family",
  foil: "Foil",
  supporting: "Supporting",
  minor: "Minor",
};

// ---------- Drafting (Phase 6) ----------

export type DraftOperation = "continue" | "rewrite" | "critique";

export interface DraftRequest {
  project_id: string;
  scene_id: string;
  operation: DraftOperation;
  instruction: string;
  selection: string | null;
  top_k_canon: number | null;
  max_voice_anchors: number | null;
  override_drift_gate: boolean;
}

export interface ChatMessage {
  role: "system" | "user" | "assistant";
  content: string;
}

export interface DraftPreview {
  messages: ChatMessage[];
  included: IncludedCategory[];
  canon_chunk_count: number;
  voice_anchor_count: number;
  current_drift: number | null;
  drift_blocks_send: boolean;
  canon_chunks: ChunkRef[];
  provider: string;
  model: string;
}

export interface DraftSuggestion {
  content: string;
  provider: string;
  model: string;
  tokens_in: number;
  tokens_out: number;
  current_drift: number | null;
  canon_chunks_used: ChunkRef[];
  override_drift_gate: boolean;
}

/** Display metadata for the canonical 15 Save the Cat beats. */
export const BEAT_META: Record<
  BeatId,
  { order: number; label: string; targetPct: number; description: string }
> = {
  "opening-image": {
    order: 0,
    label: "Opening Image",
    targetPct: 0.01,
    description: "First impression of the world and tone — opposite of Final Image.",
  },
  "theme-stated": {
    order: 1,
    label: "Theme Stated",
    targetPct: 0.05,
    description: "A side character voices the story's question or thesis.",
  },
  "set-up": {
    order: 2,
    label: "Set-Up",
    targetPct: 0.05,
    description: "Hero's flawed status quo: home, work, play, what needs fixing.",
  },
  catalyst: {
    order: 3,
    label: "Catalyst",
    targetPct: 0.1,
    description: "Life-changing event that disrupts the status quo.",
  },
  debate: {
    order: 4,
    label: "Debate",
    targetPct: 0.15,
    description: "Hero hesitates, weighs the cost, asks 'should I?'",
  },
  "break-into-two": {
    order: 5,
    label: "Break Into Two",
    targetPct: 0.2,
    description: "Hero commits and steps into the new world.",
  },
  "b-story": {
    order: 6,
    label: "B Story",
    targetPct: 0.22,
    description: "Romance/mentor subplot begins; vehicle for the theme.",
  },
  "fun-and-games": {
    order: 7,
    label: "Fun and Games",
    targetPct: 0.35,
    description: "Premise on display — the trailer moments.",
  },
  midpoint: {
    order: 8,
    label: "Midpoint",
    targetPct: 0.5,
    description: "False victory or false defeat; stakes raised.",
  },
  "bad-guys-close-in": {
    order: 9,
    label: "Bad Guys Close In",
    targetPct: 0.62,
    description: "Internal flaws + external pressure mount.",
  },
  "all-is-lost": {
    order: 10,
    label: "All Is Lost",
    targetPct: 0.75,
    description: "Lowest external moment — a 'whiff of death.'",
  },
  "dark-night-of-the-soul": {
    order: 11,
    label: "Dark Night of the Soul",
    targetPct: 0.8,
    description: "Internal collapse; hero confronts the lie.",
  },
  "break-into-three": {
    order: 12,
    label: "Break Into Three",
    targetPct: 0.85,
    description: "New plan synthesizing A and B story lessons.",
  },
  finale: {
    order: 13,
    label: "Finale",
    targetPct: 0.92,
    description: "Hero executes the plan, transforms, defeats antagonist.",
  },
  "final-image": {
    order: 14,
    label: "Final Image",
    targetPct: 0.99,
    description: "Mirror of Opening Image — proof of change.",
  },
};

export const BEAT_ORDER: BeatId[] = (
  Object.entries(BEAT_META) as Array<[BeatId, (typeof BEAT_META)[BeatId]]>
)
  .sort((a, b) => a[1].order - b[1].order)
  .map(([id]) => id);
