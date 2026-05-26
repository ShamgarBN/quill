/**
 * Manuscript view — Phase 5 MVP.
 *
 * Two columns:
 *   - left rail: scene list (create / pick / delete / reorder later)
 *   - main pane: large textarea editor, autosave (debounced 800ms),
 *                last-saved indicator, word count, and drift score.
 *
 * Storage discipline:
 *   - The scene's *content* is owned by the manuscript service (one
 *     Markdown file per scene on disk).
 *   - The scene's *metadata* (title, status, beat link) is owned by the
 *     structure service.
 *   - The editor never mutates metadata directly; it only saves content
 *     and lets the backend mirror the resulting word count back into the
 *     scene record.
 *
 * Phase 5 explicitly does NOT include:
 *   - rich text / Lexical
 *   - inline LLM completions
 *   - track-changes diffing
 * Those land in 5.x once the editor's data flow is solid.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import {
  AlertTriangle,
  CheckCircle2,
  CircleSlash,
  Loader2,
  Plus,
  Sparkles,
  Trash2,
} from "lucide-react";
import { useApp } from "@/stores/app";
import * as ipc from "@/lib/ipc";
import type { DraftOperation, DriftReport, Scene, SceneContent } from "@/types";
import { cn } from "@/lib/cn";
import { DraftingPanel } from "@/routes/DraftingPanel";
import { DiffReviewPane } from "@/components/editor/DiffReviewPane";

const AUTOSAVE_DEBOUNCE_MS = 800;
/** Don't even ask the backend for a drift score below this many words. */
const DRIFT_MIN_WORDS = 30;
/** Drift score thresholds — must match the verbal copy below. */
const DRIFT_WARN = 0.45;
const DRIFT_HIGH = 0.7;

type SaveState =
  | { kind: "idle" }
  | { kind: "dirty" }
  | { kind: "saving" }
  | { kind: "saved"; at: number }
  | { kind: "error"; message: string };

export function ManuscriptView(): JSX.Element {
  const project = useApp((s) => s.currentProject);
  const [scenes, setScenes] = useState<Scene[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);

  const [content, setContent] = useState<SceneContent | null>(null);
  const [text, setText] = useState("");
  const [save, setSave] = useState<SaveState>({ kind: "idle" });
  const [drift, setDrift] = useState<DriftReport | null>(null);
  const [hasFingerprint, setHasFingerprint] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [draftingOpen, setDraftingOpen] = useState(false);
  const [selection, setSelection] = useState<{ start: number; end: number }>({
    start: 0,
    end: 0,
  });
  /** When non-null, the editor is in track-changes review mode and the
   *  textarea is hidden; the user is reviewing an AI suggestion against
   *  the current scene text. `original` is the scene text at the moment
   *  review began; `candidate` is the full proposed scene text. */
  const [reviewMode, setReviewMode] = useState<{
    original: string;
    candidate: string;
  } | null>(null);
  const editorRef = useRef<HTMLTextAreaElement | null>(null);
  const selectionText =
    selection.end > selection.start ? text.slice(selection.start, selection.end) : "";

  // Refresh scene list whenever the project changes.
  const refreshScenes = useCallback(async () => {
    if (!project) return;
    try {
      const list = await ipc.structureScenesList(project.id);
      setScenes(list);
      // Auto-select the first scene if nothing is active.
      setActiveId((curr) => {
        if (curr && list.some((s) => s.id === curr)) return curr;
        return list[0]?.id ?? null;
      });
    } catch (e) {
      setError(messageOf(e));
    }
  }, [project]);

  useEffect(() => {
    void refreshScenes();
  }, [refreshScenes]);

  // Probe whether a voice fingerprint exists; if not, skip the drift gauge.
  useEffect(() => {
    if (!project) {
      setHasFingerprint(false);
      return;
    }
    void ipc
      .voiceFingerprint(project.id)
      .then((fp) => setHasFingerprint(fp.passage_count > 0))
      .catch(() => setHasFingerprint(false));
  }, [project]);

  // Load the active scene's content whenever it changes.
  useEffect(() => {
    if (!project || !activeId) {
      setContent(null);
      setText("");
      setSave({ kind: "idle" });
      setDrift(null);
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const c = await ipc.manuscriptLoadScene(project.id, activeId);
        if (cancelled) return;
        setContent(c);
        setText(c.text);
        setSave({ kind: "idle" });
        setDrift(null);
      } catch (e) {
        if (!cancelled) setError(messageOf(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [project, activeId]);

  // Debounced autosave: any time `text` differs from `content.text`, kick
  // off a save 800ms after the user stops typing.
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    if (!project || !activeId || !content) return;
    if (text === content.text) return;
    setSave({ kind: "dirty" });
    if (saveTimer.current) clearTimeout(saveTimer.current);
    saveTimer.current = setTimeout(() => {
      void (async () => {
        setSave({ kind: "saving" });
        try {
          const updated = await ipc.manuscriptSaveScene(project.id, activeId, text);
          setContent(updated);
          setSave({ kind: "saved", at: Date.now() });
        } catch (e) {
          setSave({ kind: "error", message: messageOf(e) });
        }
      })();
    }, AUTOSAVE_DEBOUNCE_MS);
    return () => {
      if (saveTimer.current) clearTimeout(saveTimer.current);
    };
  }, [text, content, project, activeId]);

  // Re-check drift after a successful save (cheap-ish) when there's a
  // fingerprint and enough material to make the score meaningful.
  useEffect(() => {
    if (!project || !content || !hasFingerprint) return;
    if (save.kind !== "saved") return;
    if (content.word_count < DRIFT_MIN_WORDS) {
      setDrift(null);
      return;
    }
    let cancelled = false;
    void ipc
      .voiceDrift(project.id, content.text, 8)
      .then((r) => {
        if (!cancelled) setDrift(r);
      })
      .catch(() => {
        // Not catastrophic; the editor is still usable without drift info.
      });
    return () => {
      cancelled = true;
    };
  }, [save, content, project, hasFingerprint]);

  // Manual scene CRUD handlers
  const onCreateScene = async () => {
    if (!project) return;
    try {
      const title = window.prompt("New scene title?")?.trim();
      if (!title) return;
      const s = await ipc.structureSceneCreate(project.id, title);
      await refreshScenes();
      setActiveId(s.id);
    } catch (e) {
      setError(messageOf(e));
    }
  };

  const onDeleteScene = async (id: string) => {
    if (!project) return;
    const target = scenes.find((s) => s.id === id);
    if (!target) return;
    const ok = window.confirm(
      `Delete scene "${target.title}"? The Markdown file will be removed.`,
    );
    if (!ok) return;
    try {
      await ipc.structureSceneDelete(project.id, id);
      if (activeId === id) setActiveId(null);
      await refreshScenes();
    } catch (e) {
      setError(messageOf(e));
    }
  };

  // Enter track-changes review mode for an AI suggestion. The full
  // candidate scene text is composed here from the operation:
  //  - continue: append the suggestion to the end of the scene
  //  - rewrite: splice the suggestion in where the user's selection was
  // The user then reviews sentence-by-sentence in DiffReviewPane and
  // their accepted text becomes the new scene content on Apply.
  // Hooks must be declared before any early return so React can keep a
  // stable hook order across renders (rules-of-hooks).
  const onReviewChanges = useCallback(
    (suggestion: string, operation: DraftOperation): void => {
      const trimmed = suggestion.trim();
      if (!trimmed) return;
      let candidate: string;
      if (operation === "continue") {
        const sep = text.length > 0 && !text.endsWith("\n\n") ? "\n\n" : "";
        candidate = `${text}${sep}${trimmed}\n`;
      } else if (operation === "rewrite") {
        const start = Math.min(selection.start, text.length);
        const end = Math.min(selection.end, text.length);
        if (end <= start) {
          // No selection — degrade to append rather than no-op.
          const sep = text.length > 0 && !text.endsWith("\n\n") ? "\n\n" : "";
          candidate = `${text}${sep}${trimmed}\n`;
        } else {
          candidate = text.slice(0, start) + trimmed + text.slice(end);
        }
      } else {
        // critique returns commentary, not prose to splice in.
        return;
      }
      setReviewMode({ original: text, candidate });
    },
    [text, selection],
  );

  const onApplyReview = useCallback((finalText: string): void => {
    setText(finalText);
    setReviewMode(null);
  }, []);

  const onCancelReview = useCallback((): void => {
    setReviewMode(null);
  }, []);

  if (!project) {
    return (
      <div className="flex h-full flex-col">
        <ViewHeader title="Manuscript" subtitle="No project open" />
        <div className="flex flex-1 items-center justify-center p-8 text-sm text-ink-faint">
          Open or create a project from the Projects pane to begin writing.
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <ViewHeader
        title="Manuscript"
        subtitle={project.name}
        right={
          <div className="flex items-center gap-3">
            {activeId && !draftingOpen && (
              <button
                type="button"
                onClick={() => setDraftingOpen(true)}
                className="qbtn-secondary inline-flex h-7 items-center gap-1.5 px-2.5 text-xs"
                title="Open drafting panel"
              >
                <Sparkles className="h-3.5 w-3.5" />
                Draft
              </button>
            )}
            <SaveIndicator state={save} />
          </div>
        }
      />

      {error && (
        <div className="border-b border-amber-200 bg-amber-50 px-5 py-2 text-xs text-amber-900 dark:border-amber-900/40 dark:bg-amber-950/40 dark:text-amber-200">
          {error}
        </div>
      )}

      <div className="flex flex-1 overflow-hidden">
        <SceneRail
          scenes={scenes}
          activeId={activeId}
          onPick={setActiveId}
          onCreate={onCreateScene}
          onDelete={onDeleteScene}
        />

        <div className="flex flex-1 flex-col">
          {!activeId ? (
            <EmptyState onCreate={onCreateScene} />
          ) : reviewMode ? (
            <DiffReviewPane
              original={reviewMode.original}
              candidate={reviewMode.candidate}
              onApply={onApplyReview}
              onCancel={onCancelReview}
            />
          ) : (
            <Editor
              editorRef={editorRef}
              text={text}
              onChange={setText}
              onSelectionChange={(s, e) => setSelection({ start: s, end: e })}
              wordCount={content?.word_count ?? 0}
              charCount={content?.char_count ?? 0}
              hasFingerprint={hasFingerprint}
              drift={drift}
            />
          )}
        </div>

        {draftingOpen && activeId && !reviewMode && (
          <DraftingPanel
            sceneId={activeId}
            selection={selectionText}
            onReviewChanges={onReviewChanges}
            onClose={() => setDraftingOpen(false)}
          />
        )}
      </div>
    </div>
  );
}

// ---------- Scene rail ----------

function SceneRail({
  scenes,
  activeId,
  onPick,
  onCreate,
  onDelete,
}: {
  scenes: Scene[];
  activeId: string | null;
  onPick: (id: string) => void;
  onCreate: () => void;
  onDelete: (id: string) => void;
}): JSX.Element {
  return (
    <aside className="flex w-64 shrink-0 flex-col border-r border-line-subtle bg-surface-subtle">
      <div className="flex items-center justify-between border-b border-line-subtle px-3 py-2 text-xs font-medium uppercase tracking-wide text-ink-faint">
        <span>Scenes ({scenes.length})</span>
        <button
          type="button"
          onClick={onCreate}
          className="qbtn-ghost inline-flex h-6 items-center gap-1 px-1.5 text-xs"
          title="Create scene"
        >
          <Plus className="h-3.5 w-3.5" /> New
        </button>
      </div>
      <div className="flex-1 overflow-y-auto py-1">
        {scenes.length === 0 ? (
          <div className="px-3 py-4 text-xs text-ink-faint">
            No scenes yet. Create one to start writing.
          </div>
        ) : (
          scenes.map((s) => (
            <SceneRow
              key={s.id}
              scene={s}
              active={s.id === activeId}
              onPick={() => onPick(s.id)}
              onDelete={() => onDelete(s.id)}
            />
          ))
        )}
      </div>
    </aside>
  );
}

function SceneRow({
  scene,
  active,
  onPick,
  onDelete,
}: {
  scene: Scene;
  active: boolean;
  onPick: () => void;
  onDelete: () => void;
}): JSX.Element {
  return (
    <div
      className={cn(
        "group flex items-center gap-2 px-3 py-1.5 text-sm",
        active
          ? "bg-amber-50 text-ink dark:bg-amber-950/30"
          : "hover:bg-surface-elevated",
      )}
    >
      <button
        type="button"
        onClick={onPick}
        className="flex flex-1 items-center justify-between gap-2 text-left"
      >
        <span className="truncate font-medium">
          {String(scene.order + 1).padStart(2, "0")}. {scene.title || "Untitled"}
        </span>
        <span className="shrink-0 text-[10px] text-ink-faint">
          {scene.word_count.toLocaleString()}w
        </span>
      </button>
      <button
        type="button"
        onClick={onDelete}
        className="invisible text-ink-faint hover:text-red-600 group-hover:visible"
        title="Delete scene"
      >
        <Trash2 className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}

// ---------- Editor ----------

function Editor({
  editorRef,
  text,
  onChange,
  onSelectionChange,
  wordCount,
  charCount,
  hasFingerprint,
  drift,
}: {
  editorRef: React.MutableRefObject<HTMLTextAreaElement | null>;
  text: string;
  onChange: (t: string) => void;
  onSelectionChange: (start: number, end: number) => void;
  wordCount: number;
  charCount: number;
  hasFingerprint: boolean;
  drift: DriftReport | null;
}): JSX.Element {
  const reportSelection = (el: HTMLTextAreaElement): void => {
    onSelectionChange(el.selectionStart, el.selectionEnd);
  };
  return (
    <div className="flex flex-1 flex-col">
      <div className="mx-auto flex w-full max-w-3xl flex-1 flex-col px-8 py-6">
        <textarea
          ref={editorRef}
          value={text}
          onChange={(e) => onChange(e.target.value)}
          onSelect={(e) => reportSelection(e.currentTarget)}
          onKeyUp={(e) => reportSelection(e.currentTarget)}
          onMouseUp={(e) => reportSelection(e.currentTarget)}
          spellCheck
          autoFocus
          className={cn(
            "prose-pane flex-1 resize-none border-none bg-transparent",
            "font-prose text-base leading-relaxed text-ink outline-none",
            "placeholder:text-ink-faint",
          )}
          placeholder="Begin where the silence ends…"
        />
      </div>
      <footer className="flex items-center justify-between border-t border-line-subtle bg-surface-subtle px-5 py-2 text-xs text-ink-faint">
        <div className="flex items-center gap-4">
          <span>{wordCount.toLocaleString()} words</span>
          <span>{charCount.toLocaleString()} chars</span>
        </div>
        <DriftIndicator
          hasFingerprint={hasFingerprint}
          drift={drift}
          wordCount={wordCount}
        />
      </footer>
    </div>
  );
}

function DriftIndicator({
  hasFingerprint,
  drift,
  wordCount,
}: {
  hasFingerprint: boolean;
  drift: DriftReport | null;
  wordCount: number;
}): JSX.Element {
  if (!hasFingerprint) {
    return (
      <span className="inline-flex items-center gap-1.5 text-ink-faint">
        <CircleSlash className="h-3.5 w-3.5" />
        Pin reference passages in Research to enable voice drift.
      </span>
    );
  }
  if (wordCount < DRIFT_MIN_WORDS) {
    return (
      <span className="text-ink-faint">
        Drift available after {DRIFT_MIN_WORDS} words.
      </span>
    );
  }
  if (!drift) {
    return (
      <span className="inline-flex items-center gap-1.5 text-ink-faint">
        <Loader2 className="h-3.5 w-3.5 animate-spin" /> Measuring drift…
      </span>
    );
  }

  const score = drift.drift_score;
  const tone = score >= DRIFT_HIGH ? "high" : score >= DRIFT_WARN ? "warn" : "ok";
  const label =
    score >= DRIFT_HIGH ? "off voice" : score >= DRIFT_WARN ? "drifting" : "on voice";
  const Icon = tone === "ok" ? CheckCircle2 : AlertTriangle;
  return (
    <span
      title={`Cosine similarity ${(drift.cosine * 100).toFixed(0)}%`}
      className={cn(
        "inline-flex items-center gap-1.5 font-medium",
        tone === "ok" && "text-emerald-600 dark:text-emerald-400",
        tone === "warn" && "text-amber-600 dark:text-amber-400",
        tone === "high" && "text-red-600 dark:text-red-400",
      )}
    >
      <Icon className="h-3.5 w-3.5" />
      Voice: {label} ({(score * 100).toFixed(0)})
    </span>
  );
}

function SaveIndicator({ state }: { state: SaveState }): JSX.Element {
  switch (state.kind) {
    case "idle":
      return <span className="text-xs text-ink-faint">Ready</span>;
    case "dirty":
      return <span className="text-xs text-ink-faint">Editing…</span>;
    case "saving":
      return (
        <span className="inline-flex items-center gap-1 text-xs text-ink-faint">
          <Loader2 className="h-3 w-3 animate-spin" />
          Saving
        </span>
      );
    case "saved":
      return (
        <span className="text-xs text-ink-faint">Saved {formatRelative(state.at)}</span>
      );
    case "error":
      return (
        <span className="inline-flex items-center gap-1 text-xs text-red-600">
          <AlertTriangle className="h-3 w-3" /> Save failed
        </span>
      );
  }
}

function EmptyState({ onCreate }: { onCreate: () => void }): JSX.Element {
  return (
    <div className="flex flex-1 items-center justify-center p-8">
      <div className="prose-pane max-w-prose text-center text-ink-muted">
        <p className="text-base">No scene selected.</p>
        <p className="mt-2 text-sm text-ink-subtle">
          Pick a scene from the rail, or create a new one to start drafting.
        </p>
        <button type="button" onClick={onCreate} className="qbtn-primary mt-4">
          <Plus className="mr-1.5 h-4 w-4" /> New scene
        </button>
      </div>
    </div>
  );
}

// ---------- Helpers ----------

export function ViewHeader({
  title,
  subtitle,
  right,
}: {
  title: string;
  subtitle?: string;
  right?: React.ReactNode;
}): JSX.Element {
  return (
    <header className="app-chrome flex shrink-0 items-center justify-between border-b border-line-subtle bg-surface-subtle px-5 py-3">
      <div>
        <h1 className="text-sm font-semibold text-ink">{title}</h1>
        {subtitle && <p className="mt-0.5 text-xs text-ink-faint">{subtitle}</p>}
      </div>
      {right}
    </header>
  );
}

function messageOf(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  return JSON.stringify(e);
}

function formatRelative(at: number): string {
  const delta = Date.now() - at;
  if (delta < 5_000) return "just now";
  if (delta < 60_000) return `${Math.floor(delta / 1000)}s ago`;
  if (delta < 3_600_000) return `${Math.floor(delta / 60_000)}m ago`;
  return new Date(at).toLocaleTimeString();
}
