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
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  ChevronUp,
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
  Chapter,
  ChapterPatch,
  DraftOperation,
  DriftReport,
  Scene,
  SceneContent,
  SceneStatus,
  SearchHit,
  TodayProgress,
} from "@/types";
import { AGE_BAND_GRADE_RANGE, AGE_BAND_LABEL } from "@/types";
import { cn } from "@/lib/cn";
import { fleschKincaidGrade } from "@/lib/readability";
import { DraftingPanel } from "@/routes/DraftingPanel";
import { DiffReviewPane } from "@/components/editor/DiffReviewPane";
import { SceneMetaStrip } from "@/components/editor/SceneMetaStrip";
import { PromptDialog } from "@/components/shell/PromptDialog";

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
  const [creatingScene, setCreatingScene] = useState(false);
  /** Chapter the pending new-scene dialog should create into; null =
   *  chapter of the active scene (or last chapter). */
  const [creatingSceneIn, setCreatingSceneIn] = useState<string | null>(null);
  const [chapters, setChapters] = useState<Chapter[]>([]);
  const [creatingChapter, setCreatingChapter] = useState(false);
  /** When set, the center pane shows the chapter editor instead of the
   *  scene editor. Picking a scene clears it. */
  const [activeChapterId, setActiveChapterId] = useState<string | null>(null);
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

  // Refresh chapter + scene lists whenever the project changes. Chapters
  // load first so the backend migration has run before scenes are read.
  const refreshScenes = useCallback(async () => {
    if (!project) return;
    try {
      const chapterList = await ipc.structureChaptersList(project.id);
      const list = await ipc.structureScenesList(project.id);
      setChapters(chapterList);
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
  const onCreateScene = useCallback(() => {
    if (!project) return;
    setCreatingSceneIn(null);
    setCreatingScene(true);
  }, [project]);

  const onCreateSceneInChapter = useCallback(
    (chapterId: string) => {
      if (!project) return;
      setCreatingSceneIn(chapterId);
      setCreatingScene(true);
    },
    [project],
  );

  const submitCreateScene = useCallback(
    async (title: string) => {
      if (!project) return;
      setCreatingScene(false);
      try {
        // Target: the explicitly requested chapter, else the active
        // scene's chapter, else backend default (last chapter).
        const fallback = scenes.find((s) => s.id === activeId)?.chapter_id ?? undefined;
        const s = await ipc.structureSceneCreate(
          project.id,
          title,
          undefined,
          creatingSceneIn ?? fallback,
        );
        await refreshScenes();
        setActiveChapterId(null);
        setActiveId(s.id);
      } catch (e) {
        setError(messageOf(e));
      }
    },
    [project, refreshScenes, creatingSceneIn, scenes, activeId],
  );

  // ---- Chapter handlers ----

  const submitCreateChapter = useCallback(
    async (title: string) => {
      if (!project) return;
      setCreatingChapter(false);
      try {
        const c = await ipc.structureChapterCreate(project.id, title);
        await refreshScenes();
        setActiveChapterId(c.id);
      } catch (e) {
        setError(messageOf(e));
      }
    },
    [project, refreshScenes],
  );

  const onDeleteChapter = useCallback(
    async (chapterId: string) => {
      if (!project) return;
      const ch = chapters.find((c) => c.id === chapterId);
      if (!ch) return;
      const n = scenes.filter((s) => s.chapter_id === chapterId).length;
      const msg =
        n > 0
          ? `Delete "${ch.title}"? Its ${n} scene${n === 1 ? "" : "s"} move to the neighboring chapter.`
          : `Delete empty chapter "${ch.title}"?`;
      if (!window.confirm(msg)) return;
      try {
        await ipc.structureChapterDelete(project.id, chapterId);
        if (activeChapterId === chapterId) setActiveChapterId(null);
        await refreshScenes();
      } catch (e) {
        setError(messageOf(e));
      }
    },
    [project, chapters, scenes, activeChapterId, refreshScenes],
  );

  const onPatchChapter = useCallback(
    async (chapterId: string, patch: ChapterPatch) => {
      if (!project) return;
      try {
        const updated = await ipc.structureChapterUpdate(project.id, chapterId, patch);
        setChapters((curr) => curr.map((c) => (c.id === updated.id ? updated : c)));
      } catch (e) {
        setError(messageOf(e));
      }
    },
    [project],
  );

  const onMoveChapter = useCallback(
    async (chapterId: string, dir: -1 | 1) => {
      if (!project) return;
      const ids = chapters.map((c) => c.id);
      const i = ids.indexOf(chapterId);
      const j = i + dir;
      if (i < 0 || j < 0 || j >= ids.length) return;
      const a = ids[i];
      const b = ids[j];
      if (a === undefined || b === undefined) return;
      ids[i] = b;
      ids[j] = a;
      try {
        await ipc.structureChapterReorder(project.id, ids);
        await refreshScenes();
      } catch (e) {
        setError(messageOf(e));
      }
    },
    [project, chapters, refreshScenes],
  );

  const onMoveScene = useCallback(
    async (sceneId: string, chapterId: string, index: number) => {
      if (!project) return;
      try {
        await ipc.structureSceneMove(project.id, sceneId, chapterId, index);
        await refreshScenes();
      } catch (e) {
        setError(messageOf(e));
        await refreshScenes();
      }
    },
    [project, refreshScenes],
  );

  const onExportChapter = useCallback(
    async (ch: Chapter) => {
      if (!project) return;
      setCompileStatus(null);
      try {
        const n = chapters.findIndex((c) => c.id === ch.id) + 1;
        const safe = `${project.name} ${ch.title}`
          .replace(/[^a-zA-Z0-9 _-]+/g, "_")
          .trim();
        const path = await saveDialog({
          defaultPath: `${safe || `chapter-${n}`}.md`,
          filters: [{ name: "Markdown", extensions: ["md"] }],
        });
        if (typeof path !== "string") return;
        const report = await ipc.manuscriptCompile(project.id, path, {
          only_chapter_id: ch.id,
        });
        setCompileStatus({
          kind: "ok",
          message: `Exported "${ch.title}" — ${report.scene_count} scene${report.scene_count === 1 ? "" : "s"} · ${report.word_count.toLocaleString()} words → ${path}`,
        });
      } catch (e) {
        setCompileStatus({ kind: "error", message: messageOf(e) });
      }
    },
    [project, chapters],
  );

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
          chapters={chapters}
          activeId={activeChapterId ? null : activeId}
          activeChapterId={activeChapterId}
          onPick={(id) => {
            setActiveChapterId(null);
            setActiveId(id);
          }}
          onPickChapter={setActiveChapterId}
          onCreate={onCreateScene}
          onCreateInChapter={onCreateSceneInChapter}
          onCreateChapter={() => setCreatingChapter(true)}
          onDelete={onDeleteScene}
          onDeleteChapter={(id) => void onDeleteChapter(id)}
          onMoveScene={(sceneId, chapterId, index) =>
            void onMoveScene(sceneId, chapterId, index)
          }
          onMoveChapter={(id, dir) => void onMoveChapter(id, dir)}
          onExportChapter={(ch) => void onExportChapter(ch)}
          beatSheet={beatSheet}
          visibleStatuses={visibleStatuses}
          onToggleStatus={toggleStatusFilter}
        />

        <div className="flex flex-1 flex-col">
          {activeChapterId ? (
            (() => {
              const ch = chapters.find((c) => c.id === activeChapterId);
              if (!ch) return <EmptyState onCreate={onCreateScene} />;
              const chapterWords = scenes
                .filter((s) => s.chapter_id === ch.id)
                .reduce((acc, s) => acc + s.word_count, 0);
              return (
                <ChapterEditor
                  key={ch.id}
                  chapter={ch}
                  number={chapters.findIndex((c) => c.id === ch.id) + 1}
                  words={chapterWords}
                  onPatch={(patch) => void onPatchChapter(ch.id, patch)}
                />
              );
            })()
          ) : !activeId ? (
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

        {draftingOpen && activeId && !reviewMode && !activeChapterId && (
          <DraftingPanel
            sceneId={activeId}
            selection={selectionText}
            onReviewChanges={onReviewChanges}
            onClose={() => setDraftingOpen(false)}
          />
        )}
      </div>
      {creatingScene && (
        <PromptDialog
          title="New scene"
          label="Title"
          placeholder="New scene title"
          submitLabel="Create scene"
          onSubmit={(v) => void submitCreateScene(v)}
          onCancel={() => setCreatingScene(false)}
        />
      )}
      {creatingChapter && (
        <PromptDialog
          title="New chapter"
          label="Title"
          placeholder={`e.g. "Chapter ${chapters.length + 1}" or a working title`}
          submitLabel="Create chapter"
          onSubmit={(v) => void submitCreateChapter(v)}
          onCancel={() => setCreatingChapter(false)}
        />
      )}
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
  chapters,
  activeId,
  activeChapterId,
  onPick,
  onPickChapter,
  onCreate,
  onCreateInChapter,
  onCreateChapter,
  onDelete,
  onDeleteChapter,
  onMoveScene,
  onMoveChapter,
  onExportChapter,
  beatSheet,
  visibleStatuses,
  onToggleStatus,
}: {
  scenes: Scene[];
  chapters: Chapter[];
  activeId: string | null;
  activeChapterId: string | null;
  onPick: (id: string) => void;
  onPickChapter: (id: string) => void;
  onCreate: () => void;
  onCreateInChapter: (chapterId: string) => void;
  onCreateChapter: () => void;
  onDelete: (id: string) => void;
  onDeleteChapter: (id: string) => void;
  onMoveScene: (sceneId: string, chapterId: string, index: number) => void;
  onMoveChapter: (id: string, dir: -1 | 1) => void;
  onExportChapter: (ch: Chapter) => void;
  beatSheet: BeatSheet | null;
  visibleStatuses: Set<SceneStatus>;
  onToggleStatus: (s: SceneStatus) => void;
}): JSX.Element {
  const [dragId, setDragId] = useState<string | null>(null);
  /** Scene id or `chapter:<id>` the cursor is currently over. */
  const [overKey, setOverKey] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const filteredCount = scenes.filter((s) => visibleStatuses.has(s.status)).length;

  const clearDrag = (): void => {
    setDragId(null);
    setOverKey(null);
  };

  const onDragStart =
    (id: string) =>
    (e: React.DragEvent): void => {
      setDragId(id);
      e.dataTransfer.effectAllowed = "move";
      e.dataTransfer.setData("text/plain", id);
    };
  const allowOver =
    (key: string) =>
    (e: React.DragEvent): void => {
      if (!dragId || dragId === key) return;
      e.preventDefault();
      e.dataTransfer.dropEffect = "move";
      setOverKey(key);
    };
  /** Drop before a scene row: same chapter as the target, at the target's
   *  position among the chapter's scenes (the dragged scene excluded —
   *  that matches the backend's remove-then-insert semantics). */
  const onDropOnScene =
    (target: Scene) =>
    (e: React.DragEvent): void => {
      e.preventDefault();
      const id = dragId;
      clearDrag();
      if (!id || id === target.id || !target.chapter_id) return;
      const siblings = scenes.filter(
        (s) => s.chapter_id === target.chapter_id && s.id !== id,
      );
      const index = siblings.findIndex((s) => s.id === target.id);
      if (index < 0) return;
      onMoveScene(id, target.chapter_id, index);
    };
  /** Drop on a chapter header: append to the end of that chapter. */
  const onDropOnChapter =
    (chapterId: string) =>
    (e: React.DragEvent): void => {
      e.preventDefault();
      const id = dragId;
      clearDrag();
      if (!id) return;
      const count = scenes.filter(
        (s) => s.chapter_id === chapterId && s.id !== id,
      ).length;
      onMoveScene(id, chapterId, count);
    };

  const toggleCollapse = (id: string): void => {
    setCollapsed((curr) => {
      const next = new Set(curr);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  return (
    <aside className="flex w-64 shrink-0 flex-col border-r border-line-subtle bg-surface-subtle">
      <ProgressCard scenes={scenes} beatSheet={beatSheet} />
      <div className="flex items-center justify-between border-b border-line-subtle px-3 py-2 text-xs font-medium uppercase tracking-wide text-ink-faint">
        <span>
          Scenes ({filteredCount}
          {filteredCount !== scenes.length && ` / ${scenes.length}`})
        </span>
        <div className="flex items-center gap-0.5">
          <button
            type="button"
            onClick={onCreateChapter}
            className="qbtn-ghost inline-flex h-6 items-center gap-1 px-1.5 text-xs"
            title="Create chapter"
          >
            <Plus className="h-3.5 w-3.5" /> Chapter
          </button>
          <button
            type="button"
            onClick={onCreate}
            className="qbtn-ghost inline-flex h-6 items-center gap-1 px-1.5 text-xs"
            title="Create scene (⌘N)"
          >
            <Plus className="h-3.5 w-3.5" /> Scene
          </button>
        </div>
      </div>
      <StatusFilter visible={visibleStatuses} onToggle={onToggleStatus} />
      <div className="flex-1 overflow-y-auto py-1">
        {scenes.length === 0 && chapters.length === 0 ? (
          <div className="px-3 py-4 text-xs text-ink-faint">
            No scenes yet. Create one to start writing.
          </div>
        ) : (
          chapters.map((ch, chIdx) => {
            const inChapter = scenes.filter((s) => s.chapter_id === ch.id);
            const visible = inChapter.filter((s) => visibleStatuses.has(s.status));
            const words = inChapter.reduce((acc, s) => acc + s.word_count, 0);
            const isCollapsed = collapsed.has(ch.id);
            return (
              <div key={ch.id}>
                <div
                  onDragOver={allowOver(`chapter:${ch.id}`)}
                  onDrop={onDropOnChapter(ch.id)}
                  onDragLeave={() => setOverKey(null)}
                  className={cn(
                    "group flex items-center gap-1 border-b border-line-subtle/60 px-2 py-1.5",
                    activeChapterId === ch.id && "bg-accent-subtle",
                    overKey === `chapter:${ch.id}` &&
                      "outline outline-1 outline-accent",
                  )}
                >
                  <button
                    type="button"
                    onClick={() => toggleCollapse(ch.id)}
                    className="qbtn-ghost h-5 w-5 shrink-0 p-0"
                    title={isCollapsed ? "Expand chapter" : "Collapse chapter"}
                    aria-label={isCollapsed ? "Expand chapter" : "Collapse chapter"}
                  >
                    {isCollapsed ? (
                      <ChevronRight className="h-3.5 w-3.5" />
                    ) : (
                      <ChevronDown className="h-3.5 w-3.5" />
                    )}
                  </button>
                  <button
                    type="button"
                    onClick={() => onPickChapter(ch.id)}
                    className="flex min-w-0 flex-1 flex-col items-start text-left"
                    title="Edit chapter (title, word target, hook notes)"
                  >
                    <span className="w-full truncate text-xs font-semibold text-ink">
                      {chIdx + 1}. {ch.title}
                    </span>
                    <span className="text-[10px] tabular-nums text-ink-faint">
                      {words.toLocaleString()}
                      {ch.target_word_count
                        ? ` / ${ch.target_word_count.toLocaleString()}`
                        : ""}{" "}
                      words · {inChapter.length} scene
                      {inChapter.length === 1 ? "" : "s"}
                    </span>
                  </button>
                  <div className="invisible flex items-center gap-0.5 group-hover:visible">
                    <button
                      type="button"
                      onClick={() => onMoveChapter(ch.id, -1)}
                      disabled={chIdx === 0}
                      className="qbtn-ghost h-5 w-5 p-0 disabled:opacity-30"
                      title="Move chapter up"
                      aria-label="Move chapter up"
                    >
                      <ChevronUp className="h-3 w-3" />
                    </button>
                    <button
                      type="button"
                      onClick={() => onMoveChapter(ch.id, 1)}
                      disabled={chIdx === chapters.length - 1}
                      className="qbtn-ghost h-5 w-5 p-0 disabled:opacity-30"
                      title="Move chapter down"
                      aria-label="Move chapter down"
                    >
                      <ChevronDown className="h-3 w-3" />
                    </button>
                    <button
                      type="button"
                      onClick={() => onCreateInChapter(ch.id)}
                      className="qbtn-ghost h-5 w-5 p-0"
                      title="New scene in this chapter"
                      aria-label="New scene in this chapter"
                    >
                      <Plus className="h-3 w-3" />
                    </button>
                    <button
                      type="button"
                      onClick={() => onExportChapter(ch)}
                      className="qbtn-ghost h-5 w-5 p-0"
                      title="Export this chapter to Markdown"
                      aria-label="Export this chapter"
                    >
                      <Download className="h-3 w-3" />
                    </button>
                    <button
                      type="button"
                      onClick={() => onDeleteChapter(ch.id)}
                      className="qbtn-ghost h-5 w-5 p-0 text-ink-faint hover:text-rose-600"
                      title="Delete chapter (scenes move to the neighboring chapter)"
                      aria-label="Delete chapter"
                    >
                      <Trash2 className="h-3 w-3" />
                    </button>
                  </div>
                </div>
                {!isCollapsed &&
                  (visible.length === 0 ? (
                    <div className="px-8 py-1.5 text-[11px] text-ink-faint">
                      {inChapter.length === 0
                        ? "No scenes — drop one here or use +"
                        : "No scenes match the filter."}
                    </div>
                  ) : (
                    visible.map((s) => (
                      <SceneRow
                        key={s.id}
                        scene={s}
                        active={s.id === activeId}
                        onPick={() => onPick(s.id)}
                        onDelete={() => onDelete(s.id)}
                        isDragging={dragId === s.id}
                        isDropTarget={
                          overKey === s.id && dragId !== null && dragId !== s.id
                        }
                        onDragStart={onDragStart(s.id)}
                        onDragOver={allowOver(s.id)}
                        onDrop={onDropOnScene(s)}
                        onDragEnd={clearDrag}
                      />
                    ))
                  ))}
              </div>
            );
          })
        )}
      </div>
    </aside>
  );
}

/** Center-pane editor for a chapter's pacing metadata. Debounced patches,
 *  same pattern as the Threads editor. */
function ChapterEditor({
  chapter,
  number,
  words,
  onPatch,
}: {
  chapter: Chapter;
  number: number;
  words: number;
  onPatch: (patch: ChapterPatch) => void;
}): JSX.Element {
  const [title, setTitle] = useState(chapter.title);
  const [target, setTarget] = useState(
    chapter.target_word_count != null ? String(chapter.target_word_count) : "",
  );
  const [notes, setNotes] = useState(chapter.notes);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const queue = (patch: ChapterPatch): void => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => onPatch(patch), 600);
  };

  return (
    <div className="flex flex-1 flex-col overflow-y-auto">
      <div className="mx-auto flex w-full max-w-2xl flex-col gap-5 px-6 py-6">
        <div className="text-xs font-medium uppercase tracking-wider text-ink-faint">
          Chapter {number}
        </div>
        <label className="flex flex-col gap-1.5">
          <span className="text-xs font-medium uppercase tracking-wide text-ink-faint">
            Title
          </span>
          <input
            type="text"
            value={title}
            onChange={(e) => {
              setTitle(e.target.value);
              queue({ title: e.target.value });
            }}
            className="qinput w-full text-base"
          />
        </label>
        <label className="flex flex-col gap-1.5">
          <span className="text-xs font-medium uppercase tracking-wide text-ink-faint">
            Target words ({words.toLocaleString()} written)
          </span>
          <input
            type="number"
            min={0}
            value={target}
            onChange={(e) => {
              setTarget(e.target.value);
              const n = parseInt(e.target.value, 10);
              queue({
                target_word_count: Number.isFinite(n) && n > 0 ? n : null,
              });
            }}
            placeholder="e.g. 2500 — YA chapters usually run 1.5–3k"
            className="qinput w-full"
          />
        </label>
        <label className="flex flex-col gap-1.5">
          <span className="text-xs font-medium uppercase tracking-wide text-ink-faint">
            Chapter intent / hook notes
          </span>
          <textarea
            value={notes}
            onChange={(e) => {
              setNotes(e.target.value);
              queue({ notes: e.target.value });
            }}
            rows={6}
            className="qinput w-full font-prose text-sm"
            placeholder="What must this chapter accomplish? What turn does it land on? The AI sees this while drafting — and is told to end the final scene on a hook."
          />
        </label>
        <p className="text-xs text-ink-faint">
          The AI knows each scene's position in this chapter, the running word count vs
          the target, and these notes. The chapter's final scene gets an explicit "end
          on a page-turning hook" directive.
        </p>
      </div>
    </div>
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
          <ReadabilityIndicator text={text} />
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

/** Flesch-Kincaid grade chip, scored against the project's target age
 *  band. Muted when in range, amber as a gentle nudge when outside it.
 *  Hidden below the scoring minimum (~30 words). */
function ReadabilityIndicator({ text }: { text: string }): JSX.Element | null {
  const band = useApp((s) => s.settings?.target_age_band ?? "young-adult");
  const score = useMemo(() => fleschKincaidGrade(text), [text]);
  if (!score) return null;
  const [lo, hi] = AGE_BAND_GRADE_RANGE[band];
  const inRange = score.grade >= lo && score.grade <= hi;
  return (
    <span
      title={`Flesch-Kincaid reading grade ≈ ${score.grade}. Target for ${AGE_BAND_LABEL[band]}: grades ${lo}–${hi}. A nudge, not a rule — names and dialogue skew it.`}
      className={cn(
        "inline-flex items-center gap-1 rounded-full px-1.5 py-0.5",
        inRange
          ? "text-ink-faint"
          : "bg-amber-100 text-amber-900 dark:bg-amber-900/30 dark:text-amber-200",
      )}
    >
      Grade ~{score.grade.toFixed(1)}
      {!inRange && (score.grade > hi ? " ↑" : " ↓")}
    </span>
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
