import { create } from "zustand";
import type {
  Project,
  ProjectPatch,
  Settings,
  SettingsPatch,
  ThemePreference,
} from "@/types";
import * as ipc from "@/lib/ipc";

export type RouteId =
  | "manuscript"
  | "beats"
  | "canon"
  | "bible"
  | "ideas"
  | "research"
  | "settings";

interface AppState {
  // Bootstrap
  ready: boolean;
  bootError: string | null;

  // Settings + theme
  settings: Settings | null;
  resolvedTheme: "light" | "dark";

  // Projects
  projects: Project[];
  currentProject: Project | null;

  // UI
  route: RouteId;
  sidebarCollapsed: boolean;
  focusMode: boolean;

  // Actions
  bootstrap: () => Promise<void>;
  setRoute: (r: RouteId) => void;
  toggleSidebar: () => void;
  toggleFocus: () => void;
  setTheme: (t: ThemePreference) => Promise<void>;
  updateSettings: (patch: SettingsPatch) => Promise<void>;
  refreshProjects: () => Promise<void>;
  createProject: (name: string) => Promise<Project>;
  openProject: (id: string) => Promise<void>;
  updateCurrentProject: (patch: ProjectPatch) => Promise<Project>;
}

function resolveTheme(pref: ThemePreference): "light" | "dark" {
  if (pref === "system") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }
  return pref;
}

function applyTheme(theme: "light" | "dark"): void {
  document.documentElement.classList.toggle("dark", theme === "dark");
  document.documentElement.dataset.theme = theme;
}

export const useApp = create<AppState>((set, get) => ({
  ready: false,
  bootError: null,
  settings: null,
  resolvedTheme: "light",
  projects: [],
  currentProject: null,
  route: "manuscript",
  sidebarCollapsed: false,
  focusMode: false,

  bootstrap: async () => {
    try {
      const [settings, projects] = await Promise.all([
        ipc.settingsGet(),
        ipc.projectList(),
      ]);
      const resolved = resolveTheme(settings.theme);
      applyTheme(resolved);

      window
        .matchMedia("(prefers-color-scheme: dark)")
        .addEventListener("change", (ev) => {
          if (get().settings?.theme === "system") {
            const next = ev.matches ? "dark" : "light";
            applyTheme(next);
            set({ resolvedTheme: next });
          }
        });

      set({
        ready: true,
        settings,
        projects,
        resolvedTheme: resolved,
        currentProject: projects.length === 1 ? (projects[0] ?? null) : null,
        route: projects.length === 1 ? "manuscript" : "settings",
      });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set({ ready: true, bootError: msg });
    }
  },

  setRoute: (r) => set({ route: r }),

  toggleSidebar: () => set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed })),

  toggleFocus: () => set((s) => ({ focusMode: !s.focusMode })),

  setTheme: async (t) => {
    await ipc.themeSet(t);
    const settings = await ipc.settingsGet();
    const resolved = resolveTheme(settings.theme);
    applyTheme(resolved);
    set({ settings, resolvedTheme: resolved });
  },

  updateSettings: async (patch) => {
    const settings = await ipc.settingsUpdate(patch);
    if (patch.theme !== undefined) {
      const resolved = resolveTheme(settings.theme);
      applyTheme(resolved);
      set({ resolvedTheme: resolved });
    }
    set({ settings });
  },

  refreshProjects: async () => {
    const projects = await ipc.projectList();
    set({ projects });
  },

  createProject: async (name) => {
    const project = await ipc.projectCreate(name);
    set((s) => ({
      projects: [...s.projects, project],
      currentProject: project,
      route: "manuscript",
    }));
    return project;
  },

  openProject: async (id) => {
    const project = await ipc.projectOpen(id);
    set({ currentProject: project, route: "manuscript" });
  },

  updateCurrentProject: async (patch) => {
    const current = get().currentProject;
    if (!current) {
      throw new Error("no current project to update");
    }
    const project = await ipc.projectUpdate(current.id, patch);
    set((s) => ({
      currentProject: project,
      projects: s.projects.map((p) => (p.id === project.id ? project : p)),
    }));
    return project;
  },
}));
