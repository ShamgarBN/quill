/**
 * Plot Threads — track recurring arcs that must close by Book 1's end.
 *
 * Two-pane layout:
 *   - Left rail: the list of threads, grouped by status (Open / Advancing
 *     / Resolved / Abandoned). Newest-updated first.
 *   - Center: editor for the active thread — title, description, status.
 *
 * Threads are linked to individual scenes from the Manuscript view's
 * scene-metadata strip. The drafting orchestrator pulls every
 * Open/Advancing thread on every draft so the AI knows what's in motion;
 * scene-linked ones are marked `[linked]` in the prompt.
 *
 * Resolved/Abandoned threads are kept for historical reference but are
 * excluded from AI context.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { GitBranch, Plus, Trash2 } from "lucide-react";
import { useApp } from "@/stores/app";
import * as ipc from "@/lib/ipc";
import type { Thread, ThreadPatch, ThreadStatus } from "@/types";
import { THREAD_STATUS_LABEL } from "@/types";
import { ViewHeader } from "@/routes/Manuscript";
import { cn } from "@/lib/cn";

const STATUS_OPTIONS: ThreadStatus[] = ["open", "advancing", "resolved", "abandoned"];

const STATUS_TONE: Record<ThreadStatus, string> = {
  open: "text-sky-700 dark:text-sky-300",
  advancing: "text-amber-700 dark:text-amber-300",
  resolved: "text-emerald-700 dark:text-emerald-300",
  abandoned: "text-ink-faint",
};

const STATUS_DOT: Record<ThreadStatus, string> = {
  open: "bg-sky-500",
  advancing: "bg-amber-500",
  resolved: "bg-emerald-500",
  abandoned: "bg-ink-faint",
};

const PATCH_DEBOUNCE_MS = 600;

export function ThreadsView(): JSX.Element {
  const project = useApp((s) => s.currentProject);
  const [threads, setThreads] = useState<Thread[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!project) return;
    try {
      const list = await ipc.brainThreadsList(project.id);
      list.sort(
        (a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime(),
      );
      setThreads(list);
      setActiveId((curr) => curr ?? list[0]?.id ?? null);
    } catch (e) {
      setError(messageOf(e));
    }
  }, [project]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const active = useMemo(
    () => threads.find((t) => t.id === activeId) ?? null,
    [threads, activeId],
  );

  const onCreate = async (): Promise<void> => {
    if (!project) return;
    const title = window.prompt('Thread title? (e.g. "Kaelan\'s blood-debt")')?.trim();
    if (!title) return;
    try {
      const t = await ipc.brainThreadCreate(project.id, title);
      await refresh();
      setActiveId(t.id);
    } catch (e) {
      setError(messageOf(e));
    }
  };

  const onDelete = async (id: string): Promise<void> => {
    if (!project) return;
    const t = threads.find((x) => x.id === id);
    if (!t) return;
    if (!window.confirm(`Delete thread "${t.title}"?`)) return;
    try {
      await ipc.brainThreadDelete(project.id, id);
      if (activeId === id) setActiveId(null);
      await refresh();
    } catch (e) {
      setError(messageOf(e));
    }
  };

  const onPatch = async (id: string, patch: ThreadPatch): Promise<void> => {
    if (!project) return;
    try {
      const updated = await ipc.brainThreadUpdate(project.id, id, patch);
      setThreads((curr) => curr.map((t) => (t.id === id ? updated : t)));
    } catch (e) {
      setError(messageOf(e));
    }
  };

  if (!project) {
    return (
      <div className="flex h-full flex-col">
        <ViewHeader title="Plot Threads" subtitle="No project open" />
        <div className="flex flex-1 items-center justify-center p-8 text-sm text-ink-faint">
          Open or create a project to track threads.
        </div>
      </div>
    );
  }

  const openCount = threads.filter((t) => t.status === "open").length;
  const advancingCount = threads.filter((t) => t.status === "advancing").length;

  return (
    <div className="flex h-full flex-col">
      <ViewHeader
        title="Plot Threads"
        subtitle={`${openCount} open · ${advancingCount} advancing · ${threads.length} total`}
      />
      {error && (
        <div className="border-b border-amber-200 bg-amber-50 px-5 py-2 text-xs text-amber-900 dark:border-amber-900/40 dark:bg-amber-950/40 dark:text-amber-200">
          {error}
        </div>
      )}
      <div className="flex flex-1 overflow-hidden">
        <ThreadRail
          threads={threads}
          activeId={activeId}
          onPick={setActiveId}
          onCreate={onCreate}
          onDelete={onDelete}
        />
        <div className="flex flex-1 overflow-hidden">
          {!active ? (
            <EmptyState onCreate={onCreate} />
          ) : (
            <ThreadEditor thread={active} onPatch={onPatch} />
          )}
        </div>
      </div>
    </div>
  );
}

function ThreadRail({
  threads,
  activeId,
  onPick,
  onCreate,
  onDelete,
}: {
  threads: Thread[];
  activeId: string | null;
  onPick: (id: string) => void;
  onCreate: () => void;
  onDelete: (id: string) => void;
}): JSX.Element {
  return (
    <aside className="flex w-64 shrink-0 flex-col border-r border-line-subtle bg-surface-subtle">
      <div className="flex items-center justify-between border-b border-line-subtle px-3 py-2 text-xs font-medium uppercase tracking-wide text-ink-faint">
        <span>Threads</span>
        <button
          type="button"
          onClick={onCreate}
          className="qbtn-ghost inline-flex h-6 items-center gap-1 px-1.5 text-xs"
          title="New thread"
        >
          <Plus className="h-3.5 w-3.5" /> New
        </button>
      </div>
      <div className="flex-1 overflow-y-auto py-1">
        {threads.length === 0 ? (
          <div className="px-3 py-4 text-xs text-ink-faint">No threads yet.</div>
        ) : (
          threads.map((t) => (
            <div
              key={t.id}
              className={cn(
                "group flex items-center gap-2 px-3 py-1.5 text-sm",
                t.id === activeId
                  ? "bg-amber-50 text-ink dark:bg-amber-950/30"
                  : "hover:bg-surface-elevated",
              )}
            >
              <span
                className={cn(
                  "h-1.5 w-1.5 shrink-0 rounded-full",
                  STATUS_DOT[t.status],
                )}
                title={THREAD_STATUS_LABEL[t.status]}
              />
              <button
                type="button"
                onClick={() => onPick(t.id)}
                className="flex flex-1 flex-col items-start text-left"
              >
                <span className="truncate font-medium">{t.title || "(untitled)"}</span>
                <span className={cn("truncate text-[10px]", STATUS_TONE[t.status])}>
                  {THREAD_STATUS_LABEL[t.status]}
                </span>
              </button>
              <button
                type="button"
                onClick={() => onDelete(t.id)}
                className="invisible text-ink-faint hover:text-rose-600 group-hover:visible"
                title="Delete thread"
                aria-label="Delete thread"
              >
                <Trash2 className="h-3.5 w-3.5" />
              </button>
            </div>
          ))
        )}
      </div>
    </aside>
  );
}

function ThreadEditor({
  thread,
  onPatch,
}: {
  thread: Thread;
  onPatch: (id: string, patch: ThreadPatch) => Promise<void>;
}): JSX.Element {
  // Local edit state with debounced commit so typing doesn't fire an IPC
  // call on every keystroke.
  const [title, setTitle] = useState(thread.title);
  const [description, setDescription] = useState(thread.description);

  useEffect(() => {
    setTitle(thread.title);
    setDescription(thread.description);
  }, [thread.id, thread.title, thread.description]);

  const titleTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    if (title === thread.title) return;
    if (titleTimer.current) clearTimeout(titleTimer.current);
    titleTimer.current = setTimeout(() => {
      void onPatch(thread.id, { title });
    }, PATCH_DEBOUNCE_MS);
    return () => {
      if (titleTimer.current) clearTimeout(titleTimer.current);
    };
  }, [title, thread.id, thread.title, onPatch]);

  const descTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    if (description === thread.description) return;
    if (descTimer.current) clearTimeout(descTimer.current);
    descTimer.current = setTimeout(() => {
      void onPatch(thread.id, { description });
    }, PATCH_DEBOUNCE_MS);
    return () => {
      if (descTimer.current) clearTimeout(descTimer.current);
    };
  }, [description, thread.id, thread.description, onPatch]);

  return (
    <div className="flex flex-1 flex-col overflow-y-auto">
      <div className="mx-auto flex w-full max-w-2xl flex-col gap-4 px-6 py-6">
        <div>
          <Label>Title</Label>
          <input
            type="text"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="Thread title"
            className="qinput w-full text-base"
          />
        </div>

        <div>
          <Label>Status</Label>
          <div className="mt-1 flex flex-wrap gap-1.5">
            {STATUS_OPTIONS.map((s) => {
              const active = thread.status === s;
              return (
                <button
                  key={s}
                  type="button"
                  onClick={() => void onPatch(thread.id, { status: s })}
                  className={cn(
                    "inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs transition-colors",
                    active
                      ? "border-accent bg-accent-subtle text-accent"
                      : "border-line-subtle bg-surface text-ink-muted hover:bg-surface-elevated",
                  )}
                >
                  <span className={cn("h-1.5 w-1.5 rounded-full", STATUS_DOT[s])} />
                  {THREAD_STATUS_LABEL[s]}
                </button>
              );
            })}
          </div>
          <p className="mt-1.5 text-xs text-ink-faint">
            Open/Advancing threads are injected into every draft. Resolved/Abandoned
            threads are kept for reference but excluded from AI context.
          </p>
        </div>

        <div>
          <Label>Description</Label>
          <textarea
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="What's at stake? When was it introduced? When must it close?"
            rows={8}
            className="qinput w-full resize-y font-prose text-sm"
          />
        </div>

        <p className="text-xs text-ink-faint">
          Link this thread to specific scenes from the Manuscript view — open a scene,
          click the thread chip strip below the metadata row.
        </p>
      </div>
    </div>
  );
}

function EmptyState({ onCreate }: { onCreate: () => void }): JSX.Element {
  return (
    <div className="flex flex-1 items-center justify-center p-8">
      <div className="prose-pane max-w-prose text-center text-ink-muted">
        <GitBranch className="mx-auto mb-2 h-6 w-6 opacity-60" />
        <p className="text-base">No thread selected.</p>
        <p className="mt-2 text-sm text-ink-subtle">
          Threads are recurring arcs that must close by Book 1's end — a buried grudge,
          a magic-system implication, a promise the narrator made.
        </p>
        <button type="button" onClick={onCreate} className="qbtn-primary mt-4">
          <Plus className="mr-1.5 h-4 w-4" /> New thread
        </button>
      </div>
    </div>
  );
}

function Label({ children }: { children: React.ReactNode }): JSX.Element {
  return (
    <div className="text-xs font-medium uppercase tracking-wider text-ink-faint">
      {children}
    </div>
  );
}

function messageOf(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  return JSON.stringify(e);
}
