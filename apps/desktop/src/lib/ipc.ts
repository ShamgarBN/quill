/**
 * Typed wrappers around Tauri commands.
 *
 * The frontend NEVER calls `invoke` directly with string command names from
 * components — every command goes through this file. This is the only place
 * that knows the IPC schema, which keeps the Rust ↔ TS contract in one spot.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  Project,
  Settings,
  CommitInfo,
  ThemePreference,
} from "@/types";

// ---------- Projects ----------

export const projectCreate = (name: string): Promise<Project> =>
  invoke<Project>("project_create", { name });

export const projectList = (): Promise<Project[]> =>
  invoke<Project[]>("project_list");

export const projectOpen = (id: string): Promise<Project> =>
  invoke<Project>("project_open", { id });

// ---------- Settings ----------

export const settingsGet = (): Promise<Settings> =>
  invoke<Settings>("settings_get");

export const settingsUpdate = (patch: Partial<Settings>): Promise<Settings> =>
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

export const gitCommit = (
  projectId: string,
  message?: string,
): Promise<CommitInfo> =>
  invoke<CommitInfo>("git_commit", { projectId, message: message ?? null });

export const gitLog = (
  projectId: string,
  limit = 20,
): Promise<CommitInfo[]> =>
  invoke<CommitInfo[]>("git_log", { projectId, limit });

// ---------- App info ----------

export const appInfo = (): Promise<{
  version: string;
  data_dir: string;
  phase: string;
}> => invoke("app_info");
