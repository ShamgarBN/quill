import { useEffect, useRef } from "react";
import { useApp } from "@/stores/app";
import * as ipc from "@/lib/ipc";
import { Sidebar } from "@/components/shell/Sidebar";
import { TitleBar } from "@/components/shell/TitleBar";
import { WindowDragBar } from "@/components/shell/WindowDragBar";
import { ManuscriptView } from "@/routes/Manuscript";
import { BeatsView } from "@/routes/Beats";
import { CanonView } from "@/routes/Canon";
import { BibleView } from "@/routes/Bible";
import { IdeasView } from "@/routes/Ideas";
import { ResearchView } from "@/routes/Research";
import { SettingsView } from "@/routes/Settings";
import { ProjectPicker } from "@/components/shell/ProjectPicker";
import { BootError } from "@/components/shell/BootError";
import { cn } from "@/lib/cn";

export default function App(): JSX.Element {
  const ready = useApp((s) => s.ready);
  const bootError = useApp((s) => s.bootError);
  const route = useApp((s) => s.route);
  const focusMode = useApp((s) => s.focusMode);
  const currentProject = useApp((s) => s.currentProject);
  const projects = useApp((s) => s.projects);
  const bootstrap = useApp((s) => s.bootstrap);
  const toggleFocus = useApp((s) => s.toggleFocus);

  useEffect(() => {
    void bootstrap();
  }, [bootstrap]);

  // Global keyboard shortcuts
  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      // ⌘. = toggle focus mode
      if (e.metaKey && e.key === ".") {
        e.preventDefault();
        toggleFocus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [toggleFocus]);

  // Auto-resume vault watcher: if the current project has vault_auto_watch
  // and a vault_path set, attempt to start the watcher silently. We track
  // which projects we've already attempted in this session so the effect
  // doesn't re-fire if the project metadata refreshes for other reasons.
  const autoStartedFor = useRef<Set<string>>(new Set());
  useEffect(() => {
    if (!currentProject) return;
    if (!currentProject.vault_auto_watch || !currentProject.vault_path) return;
    if (autoStartedFor.current.has(currentProject.id)) return;
    autoStartedFor.current.add(currentProject.id);
    void ipc
      .canonWatchStart(currentProject.id, currentProject.vault_path)
      .catch((e) => {
        // Vault directory may have moved/been deleted, or perms changed.
        // Don't surface as a hard error — the user can re-pick the path in
        // the Canon view if they care. Log for triage.
        console.warn("Vault watcher auto-resume failed:", e);
      });
  }, [currentProject]);

  if (!ready) {
    return <BootSplash />;
  }

  if (bootError) {
    return <BootError message={bootError} />;
  }

  // No project yet: show the picker (also accessible from the sidebar later)
  if (!currentProject && projects.length === 0) {
    return <ProjectPicker />;
  }

  return (
    <div className="flex h-full flex-col bg-surface text-ink">
      <TitleBar />
      <div className="flex min-h-0 flex-1">
        {!focusMode && <Sidebar />}
        <main
          className={cn(
            "flex min-w-0 flex-1 flex-col bg-surface",
            focusMode && "items-center",
          )}
        >
          {route === "manuscript" && <ManuscriptView />}
          {route === "beats" && <BeatsView />}
          {route === "canon" && <CanonView />}
          {route === "bible" && <BibleView />}
          {route === "ideas" && <IdeasView />}
          {route === "research" && <ResearchView />}
          {route === "settings" && <SettingsView />}
        </main>
      </div>
    </div>
  );
}

function BootSplash(): JSX.Element {
  return (
    <div className="flex h-full flex-col bg-surface text-ink-muted">
      <WindowDragBar />
      <div className="flex flex-1 items-center justify-center">
        <div className="flex items-center gap-3 text-sm">
          <div className="h-2 w-2 animate-pulse rounded-full bg-accent" />
          <span>Quill is waking up…</span>
        </div>
      </div>
    </div>
  );
}
