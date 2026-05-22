/**
 * Shared TypeScript types that mirror the Rust serde-serialized models.
 *
 * Keep this file in lockstep with `apps/desktop/src-tauri/src/models/`.
 * Field names use snake_case to match Rust default serde output.
 */

export type ThemePreference = "light" | "dark" | "system";

export type FontPreference = "charter" | "jetbrains-mono";

export type GenerationMode = "scene" | "paragraph" | "sentence";

export interface Project {
  id: string;
  name: string;
  created_at: string;       // RFC3339
  updated_at: string;       // RFC3339
  manuscript_word_count: number;
  beat_progress: number;    // 0..15
}

export interface Settings {
  theme: ThemePreference;
  prose_font: FontPreference;
  // Phase 0 placeholders; later phases extend this.
  privacy_acknowledged_at: string | null;
  default_generation_mode: GenerationMode;
  show_what_gets_sent: boolean;
}

export interface CommitInfo {
  oid: string;          // hex SHA
  short_oid: string;    // first 7 chars
  message: string;
  timestamp: string;    // RFC3339
  files_changed: number;
}

/** Discriminated error type returned by Tauri commands. */
export interface QuillError {
  kind: string;
  message: string;
}
