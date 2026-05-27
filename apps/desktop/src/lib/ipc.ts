/**
 * Typed wrappers around Tauri commands.
 *
 * The frontend NEVER calls `invoke` directly with string command names from
 * components — every command goes through this file. This is the only place
 * that knows the IPC schema, which keeps the Rust ↔ TS contract in one spot.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  AuditEntry,
  Beat,
  BeatId,
  BeatPatch,
  BeatSheet,
  CanonKind,
  Character,
  CharacterPatch,
  ChunkRef,
  ChunkSensitivity,
  CommitInfo,
  CompileOptions,
  CompileReport,
  CrossLink,
  DraftPreview,
  DraftRequest,
  DraftSuggestion,
  DriftReport,
  Idea,
  IdeaPatch,
  ImportPreview,
  IngestReport,
  Project,
  ProjectPatch,
  ProviderId,
  ProviderStatus,
  ReferencePin,
  ReferencePinPatch,
  Scene,
  SceneContent,
  ScenePatch,
  SearchHit,
  Settings,
  SettingsPatch,
  ThemePreference,
  TodayProgress,
  VoiceFeatures,
  VoiceFingerprint,
  WatchStatus,
} from "@/types";

// ---------- Projects ----------

export const projectCreate = (name: string): Promise<Project> =>
  invoke<Project>("project_create", { name });

export const projectList = (): Promise<Project[]> => invoke<Project[]>("project_list");

export const projectOpen = (id: string): Promise<Project> =>
  invoke<Project>("project_open", { id });

export const projectUpdate = (id: string, patch: ProjectPatch): Promise<Project> =>
  invoke<Project>("project_update", { id, patch });

export const projectRootPath = (id: string): Promise<string> =>
  invoke<string>("project_root_path", { id });

// ---------- Settings ----------

export const settingsGet = (): Promise<Settings> => invoke<Settings>("settings_get");

export const settingsUpdate = (patch: SettingsPatch): Promise<Settings> =>
  invoke<Settings>("settings_update", { patch });

export const themeSet = (theme: ThemePreference): Promise<void> =>
  invoke<void>("theme_set", { theme });

// ---------- Secrets (sealed at rest with AES-GCM, key derived via Argon2id) ----------

export const secretSet = (key: string, value: string): Promise<void> =>
  invoke<void>("secret_set", { key, value });

export const secretGet = (key: string): Promise<string | null> =>
  invoke<string | null>("secret_get", { key });

export const secretHas = (key: string): Promise<boolean> =>
  invoke<boolean>("secret_has", { key });

// ---------- Git (auto-commit on save) ----------

export const gitCommit = (projectId: string, message?: string): Promise<CommitInfo> =>
  invoke<CommitInfo>("git_commit", { projectId, message: message ?? null });

export const gitLog = (projectId: string, limit = 20): Promise<CommitInfo[]> =>
  invoke<CommitInfo[]>("git_log", { projectId, limit });

// ---------- Canon ingestion ----------

export const canonIngestFile = (params: {
  projectId: string;
  path: string;
  kind?: CanonKind;
  sensitivity?: ChunkSensitivity;
}): Promise<IngestReport> =>
  invoke<IngestReport>("canon_ingest_file", {
    projectId: params.projectId,
    path: params.path,
    kind: params.kind ?? null,
    sensitivity: params.sensitivity ?? null,
  });

export const canonSearch = (params: {
  projectId: string;
  query: string;
  k?: number;
  respectDoNotSend?: boolean;
}): Promise<ChunkRef[]> =>
  invoke<ChunkRef[]>("canon_search", {
    projectId: params.projectId,
    query: params.query,
    k: params.k ?? 5,
    respectDoNotSend: params.respectDoNotSend ?? true,
  });

export const canonCount = (projectId: string): Promise<number> =>
  invoke<number>("canon_count", { projectId });

// ---------- Vault watcher (Phase 5.x) ----------

export const canonWatchStart = (
  projectId: string,
  vaultPath?: string,
): Promise<WatchStatus> =>
  invoke<WatchStatus>("canon_watch_start", {
    projectId,
    vaultPath: vaultPath ?? null,
  });

export const canonWatchStop = (projectId: string): Promise<WatchStatus | null> =>
  invoke<WatchStatus | null>("canon_watch_stop", { projectId });

export const canonWatchStatus = (projectId: string): Promise<WatchStatus | null> =>
  invoke<WatchStatus | null>("canon_watch_status", { projectId });

// ---------- LLM ----------

export const llmProviderStatus = (provider: ProviderId): Promise<ProviderStatus> =>
  invoke<ProviderStatus>("llm_provider_status", { provider });

export const llmPing = (provider: ProviderId): Promise<string> =>
  invoke<string>("llm_ping", { provider });

export const auditTail = (limit = 50): Promise<AuditEntry[]> =>
  invoke<AuditEntry[]>("audit_tail", { limit });

export const auditPath = (): Promise<string> => invoke<string>("audit_path");

// ---------- Structure (beat sheet + scenes) ----------

export const structureBeatSheetGet = (projectId: string): Promise<BeatSheet> =>
  invoke<BeatSheet>("structure_beat_sheet_get", { projectId });

export const structureBeatUpdate = (
  projectId: string,
  beatId: BeatId,
  patch: BeatPatch,
): Promise<BeatSheet> =>
  invoke<BeatSheet>("structure_beat_update", { projectId, beatId, patch });

export const structureBeatSheetSetTarget = (
  projectId: string,
  targetWordCount: number,
): Promise<BeatSheet> =>
  invoke<BeatSheet>("structure_beat_sheet_set_target", {
    projectId,
    targetWordCount,
  });

export const structureBeatSheetSetFrozen = (
  projectId: string,
  frozen: boolean,
): Promise<BeatSheet> =>
  invoke<BeatSheet>("structure_beat_sheet_set_frozen", { projectId, frozen });

export const structureOutlinePreview = (text: string): Promise<ImportPreview> =>
  invoke<ImportPreview>("structure_outline_preview", { text });

export const structureOutlineApply = (
  projectId: string,
  text: string,
): Promise<BeatSheet> =>
  invoke<BeatSheet>("structure_outline_apply", { projectId, text });

export const structureScenesList = (projectId: string): Promise<Scene[]> =>
  invoke<Scene[]>("structure_scenes_list", { projectId });

export const structureSceneCreate = (
  projectId: string,
  title: string,
  beatId?: BeatId,
): Promise<Scene> =>
  invoke<Scene>("structure_scene_create", {
    projectId,
    title,
    beatId: beatId ?? null,
  });

export const structureSceneDelete = (
  projectId: string,
  sceneId: string,
): Promise<void> => invoke<void>("structure_scene_delete", { projectId, sceneId });

export const structureSceneReorder = (
  projectId: string,
  idsInOrder: string[],
): Promise<void> => invoke<void>("structure_scene_reorder", { projectId, idsInOrder });

export const structureSceneUpdate = (
  projectId: string,
  sceneId: string,
  patch: ScenePatch,
): Promise<Scene> =>
  invoke<Scene>("structure_scene_update", { projectId, sceneId, patch });

// ---------- Manuscript content (per-scene Markdown) ----------

export const manuscriptLoadScene = (
  projectId: string,
  sceneId: string,
): Promise<SceneContent> =>
  invoke<SceneContent>("manuscript_load_scene", { projectId, sceneId });

export const manuscriptSaveScene = (
  projectId: string,
  sceneId: string,
  text: string,
): Promise<SceneContent> =>
  invoke<SceneContent>("manuscript_save_scene", { projectId, sceneId, text });

export const manuscriptCompile = (
  projectId: string,
  outputPath: string | null,
  options?: CompileOptions,
): Promise<CompileReport> =>
  invoke<CompileReport>("manuscript_compile", {
    projectId,
    outputPath,
    options: options ?? null,
  });

export const manuscriptTodayProgress = (projectId: string): Promise<TodayProgress> =>
  invoke<TodayProgress>("manuscript_today_progress", { projectId });

export const manuscriptSearch = (
  projectId: string,
  query: string,
  limit?: number,
): Promise<SearchHit[]> =>
  invoke<SearchHit[]>("manuscript_search", {
    projectId,
    query,
    limit: limit ?? null,
  });

// ---------- Voice (reference pins, fingerprint, drift) ----------

export const voicePinsList = (projectId: string): Promise<ReferencePin[]> =>
  invoke<ReferencePin[]>("voice_pins_list", { projectId });

export const voicePinsCreate = (
  projectId: string,
  label: string,
  passage: string,
): Promise<ReferencePin> =>
  invoke<ReferencePin>("voice_pins_create", { projectId, label, passage });

export const voicePinsDelete = (projectId: string, id: string): Promise<void> =>
  invoke<void>("voice_pins_delete", { projectId, id });

export const voicePinsUpdate = (
  projectId: string,
  id: string,
  patch: ReferencePinPatch,
): Promise<ReferencePin> =>
  invoke<ReferencePin>("voice_pins_update", { projectId, id, patch });

export const voiceFingerprint = (projectId: string): Promise<VoiceFingerprint> =>
  invoke<VoiceFingerprint>("voice_fingerprint", { projectId });

export const voiceExtract = (text: string): Promise<VoiceFeatures> =>
  invoke<VoiceFeatures>("voice_extract", { text });

export const voiceDrift = (
  projectId: string,
  candidate: string,
  topN = 8,
): Promise<DriftReport> =>
  invoke<DriftReport>("voice_drift", { projectId, candidate, topN });

// ---------- Drafting (Phase 6) ----------

export const draftingPreview = (req: DraftRequest): Promise<DraftPreview> =>
  invoke<DraftPreview>("drafting_preview", { req });

export const draftingInvoke = (req: DraftRequest): Promise<DraftSuggestion> =>
  invoke<DraftSuggestion>("drafting_invoke", { req });

// ---------- Second brain (Phase 7) ----------

export const brainCharactersList = (projectId: string): Promise<Character[]> =>
  invoke<Character[]>("brain_characters_list", { projectId });

export const brainCharacterCreate = (
  projectId: string,
  name: string,
): Promise<Character> =>
  invoke<Character>("brain_character_create", { projectId, name });

export const brainCharacterUpdate = (
  projectId: string,
  id: string,
  patch: CharacterPatch,
): Promise<Character> =>
  invoke<Character>("brain_character_update", { projectId, id, patch });

export const brainCharacterDelete = (projectId: string, id: string): Promise<void> =>
  invoke<void>("brain_character_delete", { projectId, id });

export const brainCharacterCrossLinks = (
  projectId: string,
  id: string,
): Promise<CrossLink[]> =>
  invoke<CrossLink[]>("brain_character_cross_links", { projectId, id });

export const brainIdeasList = (projectId: string): Promise<Idea[]> =>
  invoke<Idea[]>("brain_ideas_list", { projectId });

export const brainIdeaCreate = (projectId: string, text: string): Promise<Idea> =>
  invoke<Idea>("brain_idea_create", { projectId, text });

export const brainIdeaUpdate = (
  projectId: string,
  id: string,
  patch: IdeaPatch,
): Promise<Idea> => invoke<Idea>("brain_idea_update", { projectId, id, patch });

export const brainIdeaDelete = (projectId: string, id: string): Promise<void> =>
  invoke<void>("brain_idea_delete", { projectId, id });

// `Beat` is re-exported so consumers can import alongside ipc fns.
export type { Beat };

// ---------- App info ----------

export const appInfo = (): Promise<{
  version: string;
  data_dir: string;
  phase: string;
}> => invoke("app_info");

// ---------- System integration ----------

export const systemRevealPath = (path: string): Promise<void> =>
  invoke<void>("system_reveal_path", { path });
