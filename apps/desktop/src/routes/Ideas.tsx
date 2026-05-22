/**
 * Idea Park — Phase 7.
 *
 * Capture-fast scratchpad for stray story ideas. Each idea has:
 *   - text (Markdown)
 *   - tags (free-form, comma-separated)
 *   - do_not_send flag (excludes the idea from any LLM prompt assembly)
 *
 * No filing required: tags are organic. Tag chips at the top filter
 * the list. Newest-first ordering by default.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Eye, EyeOff, Lightbulb, Plus, Send, Trash2 } from "lucide-react";
import { useApp } from "@/stores/app";
import * as ipc from "@/lib/ipc";
import type { Idea, IdeaPatch } from "@/types";
import { ViewHeader } from "@/routes/Manuscript";
import { cn } from "@/lib/cn";

const PATCH_DEBOUNCE_MS = 600;

export function IdeasView(): JSX.Element {
  const project = useApp((s) => s.currentProject);
  const [ideas, setIdeas] = useState<Idea[]>([]);
  const [draft, setDraft] = useState("");
  const [draftTags, setDraftTags] = useState("");
  const [filterTag, setFilterTag] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!project) return;
    try {
      const list = await ipc.brainIdeasList(project.id);
      list.sort(
        (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
      );
      setIdeas(list);
    } catch (e) {
      setError(messageOf(e));
    }
  }, [project]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const allTags = useMemo(() => {
    const set = new Set<string>();
    for (const i of ideas) {
      for (const t of i.tags) set.add(t);
    }
    return Array.from(set).sort();
  }, [ideas]);

  const visibleIdeas = filterTag
    ? ideas.filter((i) => i.tags.includes(filterTag))
    : ideas;

  const submitDraft = async (): Promise<void> => {
    if (!project) return;
    const trimmed = draft.trim();
    if (!trimmed) return;
    try {
      const created = await ipc.brainIdeaCreate(project.id, trimmed);
      const tags = draftTags
        .split(",")
        .map((t) => t.trim())
        .filter(Boolean);
      if (tags.length > 0) {
        await ipc.brainIdeaUpdate(project.id, created.id, { tags });
      }
      setDraft("");
      setDraftTags("");
      await refresh();
    } catch (e) {
      setError(messageOf(e));
    }
  };

  const onPatch = async (id: string, patch: IdeaPatch): Promise<void> => {
    if (!project) return;
    try {
      const updated = await ipc.brainIdeaUpdate(project.id, id, patch);
      setIdeas((curr) => curr.map((i) => (i.id === id ? updated : i)));
    } catch (e) {
      setError(messageOf(e));
    }
  };

  const onDelete = async (id: string): Promise<void> => {
    if (!project) return;
    if (!window.confirm("Delete this idea?")) return;
    try {
      await ipc.brainIdeaDelete(project.id, id);
      await refresh();
    } catch (e) {
      setError(messageOf(e));
    }
  };

  if (!project) {
    return (
      <div className="flex h-full flex-col">
        <ViewHeader title="Idea Park" subtitle="No project open" />
        <div className="flex flex-1 items-center justify-center p-8 text-sm text-ink-faint">
          Open or create a project to capture ideas.
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <ViewHeader
        title="Idea Park"
        subtitle={`${ideas.length} idea${ideas.length === 1 ? "" : "s"}`}
      />
      {error && (
        <div className="border-b border-amber-200 bg-amber-50 px-5 py-2 text-xs text-amber-900 dark:border-amber-900/40 dark:bg-amber-950/40 dark:text-amber-200">
          {error}
        </div>
      )}

      <section className="border-b border-line-subtle bg-surface-subtle px-6 py-4">
        <div className="mx-auto flex w-full max-w-2xl flex-col gap-2">
          <textarea
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
                e.preventDefault();
                void submitDraft();
              }
            }}
            placeholder="Capture a thought. ⌘↩ to save."
            rows={2}
            className="qinput resize-none font-prose text-sm"
          />
          <div className="flex items-center gap-2">
            <input
              type="text"
              value={draftTags}
              onChange={(e) => setDraftTags(e.target.value)}
              placeholder="tags (comma-separated)"
              className="qinput flex-1 text-xs"
            />
            <button
              type="button"
              onClick={() => void submitDraft()}
              disabled={draft.trim().length === 0}
              className="qbtn-primary disabled:cursor-not-allowed disabled:opacity-50"
            >
              <Plus className="mr-1.5 h-4 w-4" />
              Add
            </button>
          </div>
        </div>
      </section>

      {allTags.length > 0 && (
        <section className="flex flex-wrap gap-1.5 border-b border-line-subtle bg-surface px-6 py-2">
          <TagChip active={filterTag === null} onClick={() => setFilterTag(null)}>
            All
          </TagChip>
          {allTags.map((tag) => (
            <TagChip
              key={tag}
              active={filterTag === tag}
              onClick={() => setFilterTag(filterTag === tag ? null : tag)}
            >
              {tag}
            </TagChip>
          ))}
        </section>
      )}

      <div className="flex-1 overflow-y-auto px-6 py-4">
        <div className="mx-auto flex w-full max-w-2xl flex-col gap-2">
          {visibleIdeas.length === 0 ? (
            <div className="py-12 text-center text-sm text-ink-faint">
              <Lightbulb className="mx-auto mb-2 h-6 w-6 opacity-50" />
              {filterTag
                ? `No ideas tagged "${filterTag}"`
                : "No ideas yet — capture one above."}
            </div>
          ) : (
            visibleIdeas.map((i) => (
              <IdeaCard
                key={i.id}
                idea={i}
                onPatch={(patch) => onPatch(i.id, patch)}
                onDelete={() => onDelete(i.id)}
                onSendToScene={() => {
                  void navigator.clipboard.writeText(i.text);
                }}
              />
            ))
          )}
        </div>
      </div>
    </div>
  );
}

function TagChip({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}): JSX.Element {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "inline-flex items-center rounded-full border px-2.5 py-0.5 text-[11px] font-medium transition-colors",
        active
          ? "border-accent bg-accent-subtle text-accent"
          : "border-line-subtle bg-surface text-ink-muted hover:bg-surface-elevated",
      )}
    >
      {children}
    </button>
  );
}

function IdeaCard({
  idea,
  onPatch,
  onDelete,
  onSendToScene,
}: {
  idea: Idea;
  onPatch: (patch: IdeaPatch) => Promise<void>;
  onDelete: () => void;
  onSendToScene: () => void;
}): JSX.Element {
  const [draft, setDraft] = useState(idea.text);
  const [tagsDraft, setTagsDraft] = useState(idea.tags.join(", "));
  useEffect(() => setDraft(idea.text), [idea.text]);
  useEffect(() => setTagsDraft(idea.tags.join(", ")), [idea.tags]);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const queue = (patch: IdeaPatch): void => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => {
      void onPatch(patch);
    }, PATCH_DEBOUNCE_MS);
  };

  return (
    <article
      className={cn(
        "rounded-md border bg-surface px-3 py-2 transition-colors",
        idea.do_not_send
          ? "border-purple-300 dark:border-purple-900/50"
          : "border-line-subtle",
      )}
    >
      <textarea
        value={draft}
        onChange={(e) => {
          setDraft(e.target.value);
          queue({ text: e.target.value });
        }}
        rows={Math.max(2, draft.split("\n").length)}
        className="qinput resize-none border-none bg-transparent p-0 font-prose text-sm leading-relaxed text-ink focus:outline-none"
      />
      <div className="mt-2 flex items-center gap-2">
        <input
          type="text"
          value={tagsDraft}
          onChange={(e) => {
            setTagsDraft(e.target.value);
            const tags = e.target.value
              .split(",")
              .map((t) => t.trim())
              .filter(Boolean);
            queue({ tags });
          }}
          placeholder="tags"
          className="qinput flex-1 text-[11px]"
        />
        <button
          type="button"
          onClick={() => void onPatch({ do_not_send: !idea.do_not_send })}
          className={cn(
            "inline-flex items-center gap-1 rounded-md px-2 py-1 text-[11px]",
            idea.do_not_send
              ? "bg-purple-100 text-purple-900 dark:bg-purple-950/40 dark:text-purple-200"
              : "text-ink-muted hover:bg-surface-elevated",
          )}
          title={
            idea.do_not_send
              ? "do_not_send is on — never sent to AI"
              : "click to mark do_not_send"
          }
        >
          {idea.do_not_send ? (
            <>
              <EyeOff className="h-3.5 w-3.5" />
              don't send
            </>
          ) : (
            <>
              <Eye className="h-3.5 w-3.5" />
              sendable
            </>
          )}
        </button>
        <button
          type="button"
          onClick={onSendToScene}
          className="qbtn-ghost h-7 px-2 text-[11px]"
          title="Copy text to clipboard"
        >
          <Send className="mr-1 h-3.5 w-3.5" />
          Copy
        </button>
        <button
          type="button"
          onClick={onDelete}
          className="qbtn-ghost h-7 w-7 p-0"
          title="Delete idea"
        >
          <Trash2 className="h-3.5 w-3.5" />
        </button>
      </div>
      <div className="mt-1 flex items-center gap-2 text-[10px] text-ink-faint">
        <span>{new Date(idea.created_at).toLocaleString()}</span>
        {idea.tags.length > 0 && (
          <>
            <span>·</span>
            <span className="flex flex-wrap items-center gap-1">
              {idea.tags.map((t) => (
                <span key={t} className="rounded-full bg-surface-subtle px-1.5 py-0.5">
                  {t}
                </span>
              ))}
            </span>
          </>
        )}
      </div>
    </article>
  );
}

function messageOf(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return JSON.stringify(e);
}
