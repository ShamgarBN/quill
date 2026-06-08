/**
 * Character Bible — Phase 7.
 *
 * Three-pane layout:
 *   - left rail: list of characters, with new/delete
 *   - center: editor for the selected character (name, role, motivation,
 *     voice notes, secrets, arc one-liner)
 *   - right rail: cross-link panel showing every scene + canon chunk
 *     that mentions this character (by name or alias).
 *
 * The `secrets` field has its own `do_not_send` flag (defaults true) so
 * plot-twist material is excluded from any LLM prompt the orchestrator
 * assembles.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Eye, EyeOff, Loader2, Plus, Trash2, Users2 } from "lucide-react";
import { useApp } from "@/stores/app";
import * as ipc from "@/lib/ipc";
import type { Character, CharacterPatch, CharacterRole, CrossLink } from "@/types";
import { CHARACTER_ROLE_LABELS } from "@/types";
import { ViewHeader } from "@/routes/Manuscript";
import { PromptDialog } from "@/components/shell/PromptDialog";
import { cn } from "@/lib/cn";

const ROLE_OPTIONS: CharacterRole[] = [
  "protagonist",
  "antagonist",
  "mentor",
  "ally",
  "love-interest",
  "family",
  "foil",
  "supporting",
  "minor",
];

const PATCH_DEBOUNCE_MS = 600;

export function BibleView(): JSX.Element {
  const project = useApp((s) => s.currentProject);
  const [characters, setCharacters] = useState<Character[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [crossLinks, setCrossLinks] = useState<CrossLink[]>([]);
  const [crossLoading, setCrossLoading] = useState(false);
  const [creating, setCreating] = useState(false);

  const refreshList = useCallback(async () => {
    if (!project) return;
    try {
      const list = await ipc.brainCharactersList(project.id);
      list.sort((a, b) => a.name.localeCompare(b.name));
      setCharacters(list);
      setActiveId((curr) => curr ?? list[0]?.id ?? null);
    } catch (e) {
      setError(messageOf(e));
    }
  }, [project]);

  useEffect(() => {
    void refreshList();
  }, [refreshList]);

  const active = useMemo(
    () => characters.find((c) => c.id === activeId) ?? null,
    [characters, activeId],
  );

  // Refresh cross-links whenever the active character or project changes.
  useEffect(() => {
    if (!project || !active) {
      setCrossLinks([]);
      return;
    }
    let cancelled = false;
    setCrossLoading(true);
    void ipc
      .brainCharacterCrossLinks(project.id, active.id)
      .then((res) => {
        if (!cancelled) setCrossLinks(res);
      })
      .catch((e) => {
        if (!cancelled) setError(messageOf(e));
      })
      .finally(() => {
        if (!cancelled) setCrossLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [project, active]);

  const onCreate = (): void => {
    if (!project) return;
    setCreating(true);
  };

  const submitCreate = async (name: string): Promise<void> => {
    if (!project) return;
    setCreating(false);
    try {
      const created = await ipc.brainCharacterCreate(project.id, name);
      await refreshList();
      setActiveId(created.id);
    } catch (e) {
      setError(messageOf(e));
    }
  };

  const onDelete = async (id: string): Promise<void> => {
    if (!project) return;
    const target = characters.find((c) => c.id === id);
    if (!target) return;
    if (!window.confirm(`Delete character "${target.name}"?`)) return;
    try {
      await ipc.brainCharacterDelete(project.id, id);
      if (activeId === id) setActiveId(null);
      await refreshList();
    } catch (e) {
      setError(messageOf(e));
    }
  };

  const onPatch = async (patch: CharacterPatch): Promise<void> => {
    if (!project || !active) return;
    try {
      const updated = await ipc.brainCharacterUpdate(project.id, active.id, patch);
      setCharacters((curr) => curr.map((c) => (c.id === updated.id ? updated : c)));
    } catch (e) {
      setError(messageOf(e));
    }
  };

  if (!project) {
    return (
      <div className="flex h-full flex-col">
        <ViewHeader title="Character Bible" subtitle="No project open" />
        <div className="flex flex-1 items-center justify-center p-8 text-sm text-ink-faint">
          Open or create a project to use the Character Bible.
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <ViewHeader
        title="Character Bible"
        subtitle={`${characters.length} character${characters.length === 1 ? "" : "s"}`}
      />
      {error && (
        <div className="border-b border-amber-200 bg-amber-50 px-5 py-2 text-xs text-amber-900 dark:border-amber-900/40 dark:bg-amber-950/40 dark:text-amber-200">
          {error}
        </div>
      )}
      <div className="flex flex-1 overflow-hidden">
        <CharacterRail
          characters={characters}
          activeId={activeId}
          onPick={setActiveId}
          onCreate={onCreate}
          onDelete={onDelete}
        />
        <div className="flex flex-1 overflow-hidden">
          {!active ? (
            <EmptyState onCreate={onCreate} />
          ) : (
            <CharacterEditor character={active} onPatch={onPatch} />
          )}
        </div>
        {active && (
          <CrossLinkPanel
            character={active}
            links={crossLinks}
            loading={crossLoading}
          />
        )}
      </div>
      {creating && (
        <PromptDialog
          title="New character"
          label="Name"
          placeholder="Character name"
          submitLabel="Create character"
          onSubmit={(v) => void submitCreate(v)}
          onCancel={() => setCreating(false)}
        />
      )}
    </div>
  );
}

// ---------- Subcomponents ----------

function CharacterRail({
  characters,
  activeId,
  onPick,
  onCreate,
  onDelete,
}: {
  characters: Character[];
  activeId: string | null;
  onPick: (id: string) => void;
  onCreate: () => void;
  onDelete: (id: string) => void;
}): JSX.Element {
  return (
    <aside className="flex w-56 shrink-0 flex-col border-r border-line-subtle bg-surface-subtle">
      <div className="flex items-center justify-between border-b border-line-subtle px-3 py-2 text-xs font-medium uppercase tracking-wide text-ink-faint">
        <span>Cast</span>
        <button
          type="button"
          onClick={onCreate}
          className="qbtn-ghost inline-flex h-6 items-center gap-1 px-1.5 text-xs"
          title="New character"
        >
          <Plus className="h-3.5 w-3.5" /> New
        </button>
      </div>
      <div className="flex-1 overflow-y-auto py-1">
        {characters.length === 0 ? (
          <div className="px-3 py-4 text-xs text-ink-faint">No characters yet.</div>
        ) : (
          characters.map((c) => (
            <div
              key={c.id}
              className={cn(
                "group flex items-center gap-2 px-3 py-1.5 text-sm",
                c.id === activeId
                  ? "bg-amber-50 text-ink dark:bg-amber-950/30"
                  : "hover:bg-surface-elevated",
              )}
            >
              <button
                type="button"
                onClick={() => onPick(c.id)}
                className="flex flex-1 flex-col items-start text-left"
              >
                <span className="truncate font-medium">{c.name}</span>
                <span className="text-[10px] text-ink-faint">
                  {CHARACTER_ROLE_LABELS[c.role]}
                </span>
              </button>
              <button
                type="button"
                onClick={() => onDelete(c.id)}
                className="invisible text-ink-faint hover:text-red-600 group-hover:visible"
                title="Delete"
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

function CharacterEditor({
  character,
  onPatch,
}: {
  character: Character;
  onPatch: (patch: CharacterPatch) => Promise<void>;
}): JSX.Element {
  const [draft, setDraft] = useState<Character>(character);
  useEffect(() => setDraft(character), [character]);

  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const queuePatch = useCallback(
    (patch: CharacterPatch) => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
      debounceRef.current = setTimeout(() => {
        void onPatch(patch);
      }, PATCH_DEBOUNCE_MS);
    },
    [onPatch],
  );

  const update = (key: keyof Character, value: unknown): void => {
    setDraft((curr) => ({ ...curr, [key]: value }));
    if (key === "aliases") {
      const list = (value as string)
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean);
      queuePatch({ aliases: list });
    } else {
      queuePatch({ [key]: value } as CharacterPatch);
    }
  };

  return (
    <div className="flex flex-1 flex-col overflow-y-auto px-6 py-5">
      <div className="mx-auto flex w-full max-w-2xl flex-col gap-5">
        <Field label="Name">
          <input
            type="text"
            value={draft.name}
            onChange={(e) => update("name", e.target.value)}
            className="qinput w-full"
          />
        </Field>
        <Field label="Aliases (comma-separated)">
          <input
            type="text"
            value={draft.aliases.join(", ")}
            onChange={(e) => update("aliases", e.target.value)}
            className="qinput w-full"
            placeholder="e.g. Kael, the boy of Tarn"
          />
        </Field>
        <Field label="Role">
          <select
            value={draft.role}
            onChange={(e) => update("role", e.target.value as CharacterRole)}
            className="qinput w-full"
          >
            {ROLE_OPTIONS.map((r) => (
              <option key={r} value={r}>
                {CHARACTER_ROLE_LABELS[r]}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Arc one-liner">
          <input
            type="text"
            value={draft.arc_one_liner}
            onChange={(e) => update("arc_one_liner", e.target.value)}
            className="qinput w-full"
            placeholder="The naive farmboy who learns the cost of vengeance."
          />
        </Field>
        <Field label="Motivation">
          <textarea
            value={draft.motivation}
            onChange={(e) => update("motivation", e.target.value)}
            rows={3}
            className="qinput w-full"
            placeholder="What does this character want? Why? What's in the way?"
          />
        </Field>
        <Field label="Voice notes">
          <textarea
            value={draft.voice_notes}
            onChange={(e) => update("voice_notes", e.target.value)}
            rows={3}
            className="qinput w-full"
            placeholder="Cadence, vocabulary, tics. How they speak when angry, scared, in love."
          />
        </Field>
        <div className="rounded-md border border-line-subtle bg-surface-subtle p-3">
          <div className="flex items-center justify-between">
            <label className="text-xs font-medium uppercase tracking-wide text-ink-faint">
              Secrets / spoilers
            </label>
            <label className="inline-flex items-center gap-1.5 text-xs text-ink-muted">
              <input
                type="checkbox"
                checked={draft.secrets_do_not_send}
                onChange={(e) => update("secrets_do_not_send", e.target.checked)}
              />
              {draft.secrets_do_not_send ? (
                <>
                  <EyeOff className="h-3.5 w-3.5" />
                  Excluded from AI prompts
                </>
              ) : (
                <>
                  <Eye className="h-3.5 w-3.5" />
                  Visible to AI prompts
                </>
              )}
            </label>
          </div>
          <textarea
            value={draft.secrets}
            onChange={(e) => update("secrets", e.target.value)}
            rows={3}
            className="qinput mt-2 w-full"
            placeholder="Plot reveals, twists, things only you (and the DM) know."
          />
        </div>
      </div>
    </div>
  );
}

function CrossLinkPanel({
  character,
  links,
  loading,
}: {
  character: Character;
  links: CrossLink[];
  loading: boolean;
}): JSX.Element {
  const sceneLinks = links.filter(
    (l): l is Extract<CrossLink, { kind: "scene" }> => l.kind === "scene",
  );
  const canonLinks = links.filter(
    (l): l is Extract<CrossLink, { kind: "canon" }> => l.kind === "canon",
  );
  return (
    <aside className="flex w-72 shrink-0 flex-col border-l border-line-subtle bg-surface-subtle">
      <div className="border-b border-line-subtle px-3 py-2">
        <div className="text-xs font-semibold uppercase tracking-wide text-ink-faint">
          Mentions of {character.name}
        </div>
        <div className="text-[11px] text-ink-faint">
          {loading
            ? "scanning…"
            : `${sceneLinks.length} scene${sceneLinks.length === 1 ? "" : "s"} · ${canonLinks.length} canon`}
        </div>
      </div>
      <div className="flex-1 overflow-y-auto px-3 py-2 text-xs">
        {loading ? (
          <Loader2 className="mx-auto mt-4 h-4 w-4 animate-spin text-ink-faint" />
        ) : links.length === 0 ? (
          <p className="text-ink-faint">
            No matches yet. Add aliases or write more to see references appear.
          </p>
        ) : (
          <>
            {sceneLinks.length > 0 && (
              <section>
                <h4 className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-ink-faint">
                  Scenes
                </h4>
                <ul className="flex flex-col gap-2">
                  {sceneLinks.map((l) => (
                    <li
                      key={`${l.scene_id}-${l.location}`}
                      className="rounded-md border border-line-subtle bg-surface px-2.5 py-1.5"
                    >
                      <div className="flex items-center justify-between">
                        <span className="font-medium text-ink">
                          {String(l.order + 1).padStart(2, "0")}.{" "}
                          {l.title || "Untitled"}
                        </span>
                        <span className="text-[10px] uppercase tracking-wide text-ink-faint">
                          {l.location}
                        </span>
                      </div>
                      {l.snippet && (
                        <p className="mt-1 text-[11px] leading-relaxed text-ink-muted">
                          {l.snippet}
                        </p>
                      )}
                    </li>
                  ))}
                </ul>
              </section>
            )}
            {canonLinks.length > 0 && (
              <section className="mt-3">
                <h4 className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-ink-faint">
                  Canon
                </h4>
                <ul className="flex flex-col gap-2">
                  {canonLinks.map((l) => (
                    <li
                      key={l.chunk_id}
                      className="rounded-md border border-line-subtle bg-surface px-2.5 py-1.5"
                    >
                      <div className="text-[10px] text-ink-faint">
                        {l.headings.length > 0 ? l.headings.join(" › ") : l.doc_id}
                      </div>
                      <p className="mt-1 text-[11px] leading-relaxed text-ink-muted">
                        {l.snippet}
                      </p>
                    </li>
                  ))}
                </ul>
              </section>
            )}
          </>
        )}
      </div>
    </aside>
  );
}

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}): JSX.Element {
  return (
    <label className="flex flex-col gap-1.5">
      <span className="text-xs font-medium uppercase tracking-wide text-ink-faint">
        {label}
      </span>
      {children}
    </label>
  );
}

function EmptyState({ onCreate }: { onCreate: () => void }): JSX.Element {
  return (
    <div className="flex flex-1 items-center justify-center p-8">
      <div className="text-center text-ink-muted">
        <Users2 className="mx-auto mb-3 h-8 w-8 text-ink-faint" />
        <p className="text-base">No character selected.</p>
        <button type="button" onClick={onCreate} className="qbtn-primary mt-4">
          <Plus className="mr-1.5 h-4 w-4" /> New character
        </button>
      </div>
    </div>
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

export function PhasePlaceholder({
  phase,
  description,
}: {
  phase: number;
  description: string;
}): JSX.Element {
  return (
    <div className="flex flex-1 items-center justify-center p-8">
      <div className="max-w-md text-center text-ink-muted">
        <span className="qbadge mb-3 inline-flex">Phase {phase}</span>
        <p className="text-sm">{description}</p>
      </div>
    </div>
  );
}
