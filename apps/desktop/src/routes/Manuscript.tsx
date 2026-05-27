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
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  CheckCircle2,
  CircleSlash,
  Download,
  Loader2,
  Plus,
  Search,
  Sparkles,
  Trash2,
  X,
} from "lucide-react";
import { useApp } from "@/stores/app";
import * as ipc from "@/lib/ipc";
import type {
  BeatSheet,
  DraftOperation,
  DriftReport,
  Scene,
  SceneContent,
  SceneStatus,
  SearchHit,
  TodayProgress,
} from "@/types";
import { cn } from "@/lib/cn";
import { DraftingPanel } from "@/routes/DraftingPanel";
import { DiffReviewPane } from "@/components/editor/DiffReviewPane";
import { SceneMetaStrip } from "@/components/editor/SceneMetaStrip";

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
  /** Last-compile status: shown briefly under the header after Compile fires. */
  const [compileStatus, setCompileStatus] = useState<{
    kind: "ok" | "error";
    message: string;
  } | null>(null);
  const [compiling, setCompiling] = useState(false);
  /** Cached beat sheet so the rail header can show "X of N target words". */
  const [beatSheet, setBeatSheet] = useState<BeatSheet | null>(null);
  /** Today's writing progress; refreshes after each successful save. */
  const [todayProgress, setTodayProgress] = useState<TodayProgress | null>(null);
  /** Per-session scene status filter for the rail; hoisted so keyboard
   *  navigation can respect it. */
  const [visibleStatuses, setVisibleStatuses] = useState<Set<SceneStatus>>(
    () => new Set(ALL_STATUSES),
  );
  /** Manuscript-wide search overlay: open state + current query + results. */
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchResults, setSearchResults] = useState<SearchHit[]>([]);
  const [searching, setSearching] = useState(false);
  const searchInputRef = useRef<HTMLInputElement | null>(null);
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

  // Load the beat sheet once per project so the rail can show target progress.
  useEffect(() => {
    if (!project) {
      setBeatSheet(null);
      return;
    }
    let cancelled = false;
    void ipc
      .structureBeatSheetGet(project.id)
      .then((bs) => {
        if (!cancelled) setBeatSheet(bs);
      })
      .catch(() => {
        // Non-fatal — progress card just won't show a target.
      });
    return () => {
      cancelled = true;
    };
  }, [project]);

  // Refresh today's writing progress: once on project load, then after
  // every successful save so the "+N today" badge stays current.
  const refreshTodayProgress = useCallback(async (): Promise<void> => {
    if (!project) return;
    try {
      const p = await ipc.manuscriptTodayProgress(project.id);
      setTodayProgress(p);
    } catch {
      // Non-fatal — badge just won't update.
    }
  }, [project]);
  useEffect(() => {
    void refreshTodayProgress();
  }, [refreshTodayProgress]);

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
          void refreshTodayProgress();
        } catch (e) {
          setSave({ kind: "error", message: messageOf(e) });
        }
      })();
    }, AUTOSAVE_DEBOUNCE_MS);
    return () => {
      if (saveTimer.current) clearTimeout(saveTimer.current);
    };
  }, [text, content, project, activeId, refreshTodayProgress]);

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
  const onCreateScene = useCallback(async () => {
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
  }, [project, refreshScenes]);

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

  const toggleStatusFilter = useCallback((st: SceneStatus): void => {
    setVisibleStatuses((curr) => {
      const next = new Set(curr);
      if (next.has(st)) next.delete(st);
      else next.add(st);
      // Don't allow empty filter — re-enable all.
      if (next.size === 0) return new Set(ALL_STATUSES);
      return next;
    });
  }, []);

  // Debounced manuscript-wide search. Fires 250ms after the user stops
  // typing in the search input. Empty query → empty results, no IPC call.
  useEffect(() => {
    if (!project) return;
    if (!searchOpen) return;
    const q = searchQuery.trim();
    if (!q) {
      setSearchResults([]);
      return;
    }
    setSearching(true);
    const handle = window.setTimeout(() => {
      void ipc
        .manuscriptSearch(project.id, q, 100)
        .then((hits) => {
          setSearchResults(hits);
        })
        .catch(() => {
          // Non-fatal — leave previous results in place.
        })
        .finally(() => {
          setSearching(false);
        });
    }, 250);
    return () => window.clearTimeout(handle);
  }, [project, searchQuery, searchOpen]);

  // Focus the search input whenever the overlay opens.
  useEffect(() => {
    if (searchOpen) {
      // Defer to next tick so the input has actually mounted.
      window.requestAnimationFrame(() => {
        searchInputRef.current?.focus();
        searchInputRef.current?.select();
      });
    }
  }, [searchOpen]);

  // Scope-aware keyboard shortcuts for the Manuscript view:
  //   ⌘N → new scene
  //   ⌘[ / ⌘] → prev / next visible scene
  // Skip when focus is in a textarea/input/select so we don't intercept
  // legitimate typing or the editor's own arrow handling.
  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (!e.metaKey) return;
      const tag = (e.target as HTMLElement | null)?.tagName;
      const isFormField =
        tag === "TEXTAREA" ||
        tag === "INPUT" ||
        tag === "SELECT" ||
        (e.target as HTMLElement | null)?.isContentEditable === true;
      if (e.key === "n" && !isFormField) {
        e.preventDefault();
        void onCreateScene();
        return;
      }
      if (e.key === "f") {
        e.preventDefault();
        setSearchOpen(true);
        return;
      }
      if (e.key === "[" || e.key === "]") {
        const visible = scenes.filter((s) => visibleStatuses.has(s.status));
        if (visible.length === 0) return;
        const idx = activeId ? visible.findIndex((s) => s.id === activeId) : -1;
        const nextIdx =
          e.key === "]"
            ? Math.min(visible.length - 1, idx + 1)
            : Math.max(0, idx === -1 ? 0 : idx - 1);
        const target = visible[nextIdx];
        if (target && target.id !== activeId) {
          e.preventDefault();
          setActiveId(target.id);
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // Listing every dep keeps this fresh; onCreateScene is stable enough.
  }, [scenes, activeId, visibleStatuses, onCreateScene]);

  const onReorderScenes = useCallback(
    async (idsInOrder: string[]) => {
      if (!project) return;
      // Optimistic update so the rail doesn't flicker.
      setScenes((curr) => {
        const map = new Map(curr.map((s) => [s.id, s]));
        return idsInOrder
          .map((id, idx) => {
            const s = map.get(id);
            return s ? { ...s, order: idx } : null;
          })
          .filter((s): s is Scene => s !== null);
      });
      try {
        await ipc.structureSceneReorder(project.id, idsInOrder);
        await refreshScenes();
      } catch (e) {
        setError(messageOf(e));
        // Revert by re-pulling from disk.
        await refreshScenes();
      }
    },
    [project, refreshScenes],
  );

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

  const onCompile = useCallback(async (): Promise<void> => {
    if (!project) return;
    setCompileStatus(null);
    try {
      const defaultName = `${project.name.replace(/[^a-zA-Z0-9 _-]+/g, "_").trim() || "manuscript"}.md`;
      const path = await saveDialog({
        defaultPath: defaultName,
        filters: [{ name: "Markdown", extensions: ["md"] }],
      });
      if (typeof path !== "string") return;
      setCompiling(true);
      const report = await ipc.manuscriptCompile(project.id, path);
      setCompileStatus({
        kind: "ok",
        message: `Compiled ${report.scene_count} scene${
          report.scene_count === 1 ? "" : "s"
        } · ${report.word_count.toLocaleString()} words → ${path}`,
      });
    } catch (e) {
      setCompileStatus({ kind: "error", message: messageOf(e) });
    } finally {
      setCompiling(false);
    }
  }, [project]);

  // Auto-clear the compile status after 6 seconds.
  useEffect(() => {
    if (!compileStatus) return;
    const t = window.setTimeout(() => setCompileStatus(null), 6000);
    return () => window.clearTimeout(t);
  }, [compileStatus]);

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
            <button
              type="button"
              onClick={() => void onCompile()}
              disabled={compiling || scenes.length === 0}
              className="qbtn-ghost inline-flex h-7 items-center gap-1.5 px-2.5 text-xs disabled:opacity-50"
              title="Compile every scene in order into a single Markdown file"
            >
              {compiling ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Download className="h-3.5 w-3.5" />
              )}
              Compile
            </button>
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
            <TodaysWordsBadge progress={todayProgress} />
            <SaveIndicator state={save} />
          </div>
        }
      />

      {compileStatus && (
        <div
          className={cn(
            "border-b px-5 py-2 text-xs",
            compileStatus.kind === "ok"
              ? "border-emerald-200 bg-emerald-50 text-emerald-900 dark:border-emerald-900/40 dark:bg-emerald-950/40 dark:text-emerald-200"
              : "border-rose-200 bg-rose-50 text-rose-900 dark:border-rose-900/40 dark:bg-rose-950/40 dark:text-rose-200",
          )}
        >
          {compileStatus.message}
        </div>
      )}

      <VaultPrivacyBanner />

      {searchOpen && (
        <SearchOverlay
          inputRef={searchInputRef}
          query={searchQuery}
          onQueryChange={setSearchQuery}
          results={searchResults}
          searching={searching}
          onPick={(hit) => {
            setActiveId(hit.scene_id);
            setSearchOpen(false);
          }}
          onClose={() => setSearchOpen(false)}
        />
      )}

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
          onReorder={onReorderScenes}
          beatSheet={beatSheet}
          visibleStatuses={visibleStatuses}
          onToggleStatus={toggleStatusFilter}
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
            <>
              {(() => {
                const activeScene = scenes.find((s) => s.id === activeId);
                if (!activeScene || !project) return null;
                return (
                  <SceneMetaStrip
                    projectId={project.id}
                    scene={activeScene}
                    scenePath={content?.path}
                    onSceneUpdated={(updated) =>
                      setScenes((curr) =>
                        curr.map((s) => (s.id === updated.id ? updated : s)),
                      )
                    }
                  />
                );
              })()}
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
            </>
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

/** Default: all statuses visible. */
const ALL_STATUSES: SceneStatus[] = [
  "outlined",
  "drafting",
  "drafted",
  "revised",
  "locked",
];

function SceneRail({
  scenes,
  activeId,
  onPick,
  onCreate,
  onDelete,
  onReorder,
  beatSheet,
  visibleStatuses,
  onToggleStatus,
}: {
  scenes: Scene[];
  activeId: string | null;
  onPick: (id: string) => void;
  onCreate: () => void;
  onDelete: (id: string) => void;
  onReorder: (idsInOrder: string[]) => void;
  beatSheet: BeatSheet | null;
  visibleStatuses: Set<SceneStatus>;
  onToggleStatus: (s: SceneStatus) => void;
}): JSX.Element {
  const [dragId, setDragId] = useState<string | null>(null);
  const [overId, setOverId] = useState<string | null>(null);
  const filteredScenes = scenes.filter((s) => visibleStatuses.has(s.status));

  const onDragStart =
    (id: string) =>
    (e: React.DragEvent): void => {
      setDragId(id);
      e.dataTransfer.effectAllowed = "move";
      e.dataTransfer.setData("text/plain", id);
    };
  const onDragOver =
    (id: string) =>
    (e: React.DragEvent): void => {
      if (!dragId || dragId === id) return;
      e.preventDefault();
      e.dataTransfer.dropEffect = "move";
      setOverId(id);
    };
  const onDrop =
    (targetId: string) =>
    (e: React.DragEvent): void => {
      e.preventDefault();
      if (!dragId || dragId === targetId) {
        setDragId(null);
        setOverId(null);
        return;
      }
      // Compute the new order: remove dragId from its current slot, then
      // insert it immediately before targetId.
      const ids = scenes.map((s) => s.id);
      const from = ids.indexOf(dragId);
      const to = ids.indexOf(targetId);
      if (from < 0 || to < 0) {
        setDragId(null);
        setOverId(null);
        return;
      }
      ids.splice(from, 1);
      const insertAt = from < to ? to - 1 : to;
      ids.splice(insertAt, 0, dragId);
      setDragId(null);
      setOverId(null);
      onReorder(ids);
    };
  const onDragEnd = (): void => {
    setDragId(null);
    setOverId(null);
  };

  return (
    <aside className="flex w-64 shrink-0 flex-col border-r border-line-subtle bg-surface-subtle">
      <ProgressCard scenes={scenes} beatSheet={beatSheet} />
      <div className="flex items-center justify-between border-b border-line-subtle px-3 py-2 text-xs font-medium uppercase tracking-wide text-ink-faint">
        <span>
          Scenes ({filteredScenes.length}
          {filteredScenes.length !== scenes.length && ` / ${scenes.length}`})
        </span>
        <button
          type="button"
          onClick={onCreate}
          className="qbtn-ghost inline-flex h-6 items-center gap-1 px-1.5 text-xs"
          title="Create scene"
        >
          <Plus className="h-3.5 w-3.5" /> New
        </button>
      </div>
      <StatusFilter visible={visibleStatuses} onToggle={onToggleStatus} />
      <div className="flex-1 overflow-y-auto py-1">
        {filteredScenes.length === 0 ? (
          <div className="px-3 py-4 text-xs text-ink-faint">
            {scenes.length === 0
              ? "No scenes yet. Create one to start writing."
              : "No scenes match the current filter."}
          </div>
        ) : (
          filteredScenes.map((s) => (
            <SceneRow
              key={s.id}
              scene={s}
              active={s.id === activeId}
              onPick={() => onPick(s.id)}
              onDelete={() => onDelete(s.id)}
              isDragging={dragId === s.id}
              isDropTarget={overId === s.id && dragId !== null && dragId !== s.id}
              onDragStart={onDragStart(s.id)}
              onDragOver={onDragOver(s.id)}
              onDrop={onDrop(s.id)}
              onDragEnd={onDragEnd}
            />
          ))
        )}
      </div>
    </aside>
  );
}

function SearchOverlay({
  inputRef,
  query,
  onQueryChange,
  results,
  searching,
  onPick,
  onClose,
}: {
  inputRef: React.RefObject<HTMLInputElement>;
  query: string;
  onQueryChange: (q: string) => void;
  results: SearchHit[];
  searching: boolean;
  onPick: (hit: SearchHit) => void;
  onClose: () => void;
}): JSX.Element {
  return (
    <div className="border-b border-line-subtle bg-surface-subtle">
      <div className="flex items-center gap-2 px-4 py-2">
        <Search className="h-4 w-4 text-ink-faint" />
        <input
          ref={inputRef}
          type="text"
          value={query}
          onChange={(e) => onQueryChange(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              e.preventDefault();
              onClose();
            } else if (e.key === "Enter" && results[0]) {
              e.preventDefault();
              onPick(results[0]);
            }
          }}
          placeholder="Search manuscript… (Esc to close, Enter to jump to first match)"
          className="flex-1 bg-transparent text-sm outline-none placeholder:text-ink-faint"
        />
        {searching && <Loader2 className="h-3.5 w-3.5 animate-spin text-ink-faint" />}
        <span className="text-xs text-ink-faint">
          {results.length} match{results.length === 1 ? "" : "es"}
        </span>
        <button
          type="button"
          onClick={onClose}
          className="qbtn-ghost h-7 w-7 p-0"
          title="Close search"
          aria-label="Close search"
        >
          <X className="h-3.5 w-3.5" />
        </button>
      </div>
      {results.length > 0 && (
        <ul className="max-h-72 divide-y divide-line-subtle overflow-y-auto border-t border-line-subtle">
          {results.map((hit, i) => (
            <li key={`${hit.scene_id}-${i}`}>
              <button
                type="button"
                onClick={() => onPick(hit)}
                className="flex w-full items-baseline gap-3 px-4 py-1.5 text-left text-xs hover:bg-surface-elevated"
              >
                <span className="shrink-0 font-medium text-ink-muted">
                  {String(hit.scene_order + 1).padStart(2, "0")}.{" "}
                  {hit.scene_title || "Untitled"}
                </span>
                <span className="shrink-0 text-ink-faint tabular-nums">
                  L{hit.line}
                </span>
                <span className="min-w-0 flex-1 truncate text-ink">{hit.snippet}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

const STATUS_DOT: Record<SceneStatus, string> = {
  outlined: "bg-ink-faint",
  drafting: "bg-amber-500",
  drafted: "bg-sky-500",
  revised: "bg-emerald-500",
  locked: "bg-ink",
};

const STATUS_LABEL: Record<SceneStatus, string> = {
  outlined: "Outlined",
  drafting: "Drafting",
  drafted: "Drafted",
  revised: "Revised",
  locked: "Locked",
};

function StatusFilter({
  visible,
  onToggle,
}: {
  visible: Set<SceneStatus>;
  onToggle: (s: SceneStatus) => void;
}): JSX.Element {
  return (
    <div className="flex flex-wrap items-center gap-1 border-b border-line-subtle px-2 py-1.5">
      {ALL_STATUSES.map((st) => {
        const on = visible.has(st);
        return (
          <button
            key={st}
            type="button"
            onClick={() => onToggle(st)}
            title={`${on ? "Hide" : "Show"} ${STATUS_LABEL[st]} scenes`}
            className={cn(
              "inline-flex items-center gap-1 rounded-full border px-1.5 py-0.5 text-[10px] transition-colors",
              on
                ? "border-line-subtle bg-surface text-ink-muted"
                : "border-line-subtle bg-transparent text-ink-faint opacity-50",
            )}
          >
            <span className={cn("h-1.5 w-1.5 rounded-full", STATUS_DOT[st])} />
            {STATUS_LABEL[st]}
          </button>
        );
      })}
    </div>
  );
}

function ProgressCard({
  scenes,
  beatSheet,
}: {
  scenes: Scene[];
  beatSheet: BeatSheet | null;
}): JSX.Element | null {
  if (scenes.length === 0 && !beatSheet) return null;
  const totalWords = scenes.reduce((acc, s) => acc + s.word_count, 0);
  const target = beatSheet?.target_word_count ?? 0;
  const pct = target > 0 ? Math.min(100, Math.round((totalWords / target) * 100)) : 0;
  // Count distinct beat IDs assigned to scenes (scenes can share beats).
  const assigned = new Set(
    scenes
      .map((s) => s.beat_id)
      .filter((id): id is NonNullable<typeof id> => id !== null),
  );
  const beatsCovered = assigned.size;
  const beatsTotal = beatSheet?.beats.length ?? 15;

  return (
    <div className="border-b border-line-subtle px-3 py-2.5">
      <div className="flex items-baseline justify-between text-xs">
        <span className="font-semibold text-ink">
          {totalWords.toLocaleString()}
          {target > 0 && (
            <span className="font-normal text-ink-faint">
              {" "}
              / {target.toLocaleString()}
            </span>
          )}
        </span>
        <span className="text-ink-faint">{target > 0 ? `${pct}%` : "words"}</span>
      </div>
      {target > 0 && (
        <div className="mt-1.5 h-1 w-full overflow-hidden rounded-full bg-line-subtle">
          <div
            className={cn(
              "h-full transition-all",
              pct >= 100 ? "bg-emerald-500" : pct >= 50 ? "bg-amber-500" : "bg-sky-500",
            )}
            style={{ width: `${pct}%` }}
          />
        </div>
      )}
      <div className="mt-1.5 text-[10px] uppercase tracking-wide text-ink-faint">
        {beatsCovered}/{beatsTotal} beats touched
      </div>
    </div>
  );
}

function SceneRow({
  scene,
  active,
  onPick,
  onDelete,
  isDragging,
  isDropTarget,
  onDragStart,
  onDragOver,
  onDrop,
  onDragEnd,
}: {
  scene: Scene;
  active: boolean;
  onPick: () => void;
  onDelete: () => void;
  isDragging: boolean;
  isDropTarget: boolean;
  onDragStart: (e: React.DragEvent) => void;
  onDragOver: (e: React.DragEvent) => void;
  onDrop: (e: React.DragEvent) => void;
  onDragEnd: () => void;
}): JSX.Element {
  return (
    <div
      draggable
      onDragStart={onDragStart}
      onDragOver={onDragOver}
      onDrop={onDrop}
      onDragEnd={onDragEnd}
      className={cn(
        "group flex cursor-grab items-center gap-2 border-t-2 border-transparent px-3 py-1.5 text-sm",
        active
          ? "bg-amber-50 text-ink dark:bg-amber-950/30"
          : "hover:bg-surface-elevated",
        isDragging && "opacity-40",
        isDropTarget && "border-t-accent",
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

function VaultPrivacyBanner(): JSX.Element | null {
  const project = useApp((s) => s.currentProject);
  const settings = useApp((s) => s.settings);
  const setRoute = useApp((s) => s.setRoute);

  if (!project || !settings) return null;
  if (!project.vault_path) return null;
  const cloudActive =
    settings.chat_provider !== "mock" || settings.embedding_provider !== "mock";
  if (!cloudActive) return null;
  // Banner only when the safety net is missing: no rules AND default is Public.
  const hasRules = project.vault_rules.length > 0;
  const defaultIsPublic = project.vault_default_sensitivity === "public";
  if (hasRules || !defaultIsPublic) return null;

  return (
    <div className="flex items-start gap-2 border-b border-amber-300 bg-amber-50 px-5 py-2 text-xs text-amber-900 dark:border-amber-900/40 dark:bg-amber-950/40 dark:text-amber-100">
      <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
      <div className="flex-1">
        <span className="font-medium">Your vault is auto-syncing as Public.</span> Every
        file is eligible to be sent to{" "}
        <span className="font-medium">{settings.chat_provider}</span>. Add folder rules
        before drafting, or restrict to Mock providers.
      </div>
      <button
        type="button"
        onClick={() => setRoute("canon")}
        className="qbtn-ghost h-6 shrink-0 px-2 text-xs"
      >
        Configure rules →
      </button>
    </div>
  );
}

function TodaysWordsBadge({
  progress,
}: {
  progress: TodayProgress | null;
}): JSX.Element | null {
  if (!progress || progress.delta === 0) return null;
  const sign = progress.delta > 0 ? "+" : "";
  const tone =
    progress.delta > 0
      ? "text-emerald-700 dark:text-emerald-300"
      : "text-rose-700 dark:text-rose-300";
  const yesterday =
    progress.previous_delta !== null && progress.previous_delta !== 0
      ? `Yesterday: ${progress.previous_delta > 0 ? "+" : ""}${progress.previous_delta.toLocaleString()}`
      : undefined;
  return (
    <span
      className={cn("text-xs tabular-nums", tone)}
      title={yesterday ?? "Words written today"}
    >
      {sign}
      {progress.delta.toLocaleString()} today
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
