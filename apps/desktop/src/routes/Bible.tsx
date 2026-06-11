/**
 * World Bible — the home for every canonical entity, in tabs:
 *   - Characters: three-pane (rail · editor · cross-link panel). The
 *     `secrets` field has its own `do_not_send` flag (default true) so
 *     plot-twist material is kept out of LLM prompts.
 *   - Places / Factions / Lore: two-pane (rail · editor) over WorldEntry
 *     records, discriminated by kind.
 *
 * All four are auto-populated by the canon extraction pass (entries show
 * an "AI" badge and refetch live on `canon-extraction-complete`) and are
 * fully hand-editable.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Eye, EyeOff, Globe2, Loader2, Plus, Trash2, Users2 } from "lucide-react";
import { useApp } from "@/stores/app";
import * as ipc from "@/lib/ipc";
import type {
  Character,
  CharacterPatch,
  CharacterRole,
  CrossLink,
  WorldEntry,
  WorldEntryPatch,
  WorldKind,
} from "@/types";
import { CHARACTER_ROLE_LABELS, WORLD_KIND_LABEL } from "@/types";
import { ViewHeader } from "@/routes/Manuscript";
import { PromptDialog } from "@/components/shell/PromptDialog";
import { AISuggestedBadge } from "@/components/shell/AISuggestedBadge";
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

type BibleTab = "characters" | "location" | "faction" | "lore";

const WORLD_TABS: { id: BibleTab; label: string; kind: WorldKind }[] = [
  { id: "location", label: "Places", kind: "location" },
  { id: "faction", label: "Factions", kind: "faction" },
  { id: "lore", label: "Lore", kind: "lore" },
];

/**
 * World Bible shell: a tabbed home for every canonical entity.
 *   - Characters → the original three-pane Character Bible.
 *   - Places / Factions / Lore → World entries (locations, organizations,
 *     and the rules/myths of the setting), each a two-pane rail+editor.
 *
 * All four are populated by the canon extraction pass; entries carry an
 * "AI" badge and refetch live when an extraction completes.
 */
export function BibleView(): JSX.Element {
  const project = useApp((s) => s.currentProject);
  const [tab, setTab] = useState<BibleTab>("characters");
  const [counts, setCounts] = useState({
    characters: 0,
    location: 0,
    faction: 0,
    lore: 0,
  });

  const refreshCounts = useCallback(async () => {
    if (!project) return;
    try {
      const [chars, world] = await Promise.all([
        ipc.brainCharactersList(project.id),
        ipc.brainWorldList(project.id),
      ]);
      setCounts({
        characters: chars.length,
        location: world.filter((w) => w.kind === "location").length,
        faction: world.filter((w) => w.kind === "faction").length,
        lore: world.filter((w) => w.kind === "lore").length,
      });
    } catch {
      // Non-fatal — tab badges just won't update.
    }
  }, [project]);

  useEffect(() => {
    void refreshCounts();
  }, [refreshCounts]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen("canon-extraction-complete", () => {
      void refreshCounts();
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      if (unlisten) unlisten();
    };
  }, [refreshCounts]);

  if (!project) {
    return (
      <div className="flex h-full flex-col">
        <ViewHeader title="Bible" subtitle="No project open" />
        <div className="flex flex-1 items-center justify-center p-8 text-sm text-ink-faint">
          Open or create a project to use the Bible.
        </div>
      </div>
    );
  }

  const total = counts.characters + counts.location + counts.faction + counts.lore;

  return (
    <div className="flex h-full flex-col">
      <ViewHeader
        title="Bible"
        subtitle={`${total} ${total === 1 ? "entry" : "entries"} across people, places, factions & lore`}
      />
      <div className="flex shrink-0 items-center gap-1 border-b border-line-subtle bg-surface-subtle px-3 py-1.5">
        <TabButton
          label="Characters"
          count={counts.characters}
          active={tab === "characters"}
          onClick={() => setTab("characters")}
        />
        {WORLD_TABS.map((t) => (
          <TabButton
            key={t.id}
            label={t.label}
            count={counts[t.id as "location" | "faction" | "lore"]}
            active={tab === t.id}
            onClick={() => setTab(t.id)}
          />
        ))}
      </div>
      <div className="flex flex-1 overflow-hidden">
        {tab === "characters" ? (
          <CharactersTab project={project} onMutate={refreshCounts} />
        ) : (
          <WorldTab
            project={project}
            kind={tab as WorldKind}
            onMutate={refreshCounts}
          />
        )}
      </div>
    </div>
  );
}

function TabButton({
  label,
  count,
  active,
  onClick,
}: {
  label: string;
  count: number;
  active: boolean;
  onClick: () => void;
}): JSX.Element {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "inline-flex items-center gap-1.5 rounded-md px-3 py-1 text-xs font-medium transition-colors",
        active
          ? "bg-accent-subtle text-accent"
          : "text-ink-muted hover:bg-surface-muted hover:text-ink",
      )}
    >
      {label}
      <span
        className={cn(
          "rounded-full px-1.5 text-[10px] tabular-nums",
          active ? "bg-accent/20 text-accent" : "bg-surface-muted text-ink-faint",
        )}
      >
        {count}
      </span>
    </button>
  );
}

// ---------- Characters tab ----------

function CharactersTab({
  project,
  onMutate,
}: {
  project: { id: string };
  onMutate: () => void;
}): JSX.Element {
  const [characters, setCharacters] = useState<Character[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [crossLinks, setCrossLinks] = useState<CrossLink[]>([]);
  const [crossLoading, setCrossLoading] = useState(false);
  const [creating, setCreating] = useState(false);

  const refreshList = useCallback(async () => {
    try {
      const list = await ipc.brainCharactersList(project.id);
      list.sort((a, b) => a.name.localeCompare(b.name));
      setCharacters(list);
      setActiveId((curr) => curr ?? list[0]?.id ?? null);
    } catch (e) {
      setError(messageOf(e));
    }
  }, [project.id]);

  useEffect(() => {
    void refreshList();
  }, [refreshList]);

  // Refresh when the canon extraction pass adds new characters.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen("canon-extraction-complete", () => {
      void refreshList();
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      if (unlisten) unlisten();
    };
  }, [refreshList]);

  const active = useMemo(
    () => characters.find((c) => c.id === activeId) ?? null,
    [characters, activeId],
  );

  // Refresh cross-links whenever the active character or project changes.
  useEffect(() => {
    if (!active) {
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
  }, [project.id, active]);

  const onCreate = (): void => setCreating(true);

  const submitCreate = async (name: string): Promise<void> => {
    setCreating(false);
    try {
      const created = await ipc.brainCharacterCreate(project.id, name);
      await refreshList();
      onMutate();
      setActiveId(created.id);
    } catch (e) {
      setError(messageOf(e));
    }
  };

  const onDelete = async (id: string): Promise<void> => {
    const target = characters.find((c) => c.id === id);
    if (!target) return;
    if (!window.confirm(`Delete character "${target.name}"?`)) return;
    try {
      await ipc.brainCharacterDelete(project.id, id);
      if (activeId === id) setActiveId(null);
      await refreshList();
      onMutate();
    } catch (e) {
      setError(messageOf(e));
    }
  };

  const onPatch = async (patch: CharacterPatch): Promise<void> => {
    if (!active) return;
    try {
      const updated = await ipc.brainCharacterUpdate(project.id, active.id, patch);
      setCharacters((curr) => curr.map((c) => (c.id === updated.id ? updated : c)));
    } catch (e) {
      setError(messageOf(e));
    }
  };

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
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

// ---------- World tab (places / factions / lore) ----------

function WorldTab({
  project,
  kind,
  onMutate,
}: {
  project: { id: string };
  kind: WorldKind;
  onMutate: () => void;
}): JSX.Element {
  const [entries, setEntries] = useState<WorldEntry[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  const refreshList = useCallback(async () => {
    try {
      const list = await ipc.brainWorldList(project.id);
      list.sort((a, b) => a.name.localeCompare(b.name));
      setEntries(list);
    } catch (e) {
      setError(messageOf(e));
    }
  }, [project.id]);

  useEffect(() => {
    void refreshList();
  }, [refreshList]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen("canon-extraction-complete", () => {
      void refreshList();
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      if (unlisten) unlisten();
    };
  }, [refreshList]);

  const visible = useMemo(
    () => entries.filter((e) => e.kind === kind),
    [entries, kind],
  );

  // Reset selection when switching tabs (kind changes).
  useEffect(() => {
    setActiveId((curr) => {
      if (curr && visible.some((e) => e.id === curr)) return curr;
      return visible[0]?.id ?? null;
    });
  }, [kind, visible]);

  const active = useMemo(
    () => visible.find((e) => e.id === activeId) ?? null,
    [visible, activeId],
  );

  const onCreate = (): void => setCreating(true);

  const submitCreate = async (name: string): Promise<void> => {
    setCreating(false);
    try {
      const created = await ipc.brainWorldCreate(project.id, name, kind);
      await refreshList();
      onMutate();
      setActiveId(created.id);
    } catch (e) {
      setError(messageOf(e));
    }
  };

  const onDelete = async (id: string): Promise<void> => {
    const target = entries.find((e) => e.id === id);
    if (!target) return;
    if (!window.confirm(`Delete "${target.name}"?`)) return;
    try {
      await ipc.brainWorldDelete(project.id, id);
      if (activeId === id) setActiveId(null);
      await refreshList();
      onMutate();
    } catch (e) {
      setError(messageOf(e));
    }
  };

  const onPatch = async (id: string, patch: WorldEntryPatch): Promise<void> => {
    try {
      const updated = await ipc.brainWorldUpdate(project.id, id, patch);
      setEntries((curr) => curr.map((e) => (e.id === updated.id ? updated : e)));
    } catch (e) {
      setError(messageOf(e));
    }
  };

  const label = WORLD_KIND_LABEL[kind];

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      {error && (
        <div className="border-b border-amber-200 bg-amber-50 px-5 py-2 text-xs text-amber-900 dark:border-amber-900/40 dark:bg-amber-950/40 dark:text-amber-200">
          {error}
        </div>
      )}
      <div className="flex flex-1 overflow-hidden">
        <WorldRail
          entries={visible}
          label={label}
          activeId={activeId}
          onPick={setActiveId}
          onCreate={onCreate}
          onDelete={onDelete}
        />
        <div className="flex flex-1 overflow-hidden">
          {!active ? (
            <WorldEmptyState kind={kind} onCreate={onCreate} />
          ) : (
            <WorldEditor
              key={active.id}
              entry={active}
              onPatch={(patch) => onPatch(active.id, patch)}
            />
          )}
        </div>
      </div>
      {creating && (
        <PromptDialog
          title={`New ${label.toLowerCase()}`}
          label="Name"
          placeholder={`${label} name`}
          submitLabel={`Create ${label.toLowerCase()}`}
          onSubmit={(v) => void submitCreate(v)}
          onCancel={() => setCreating(false)}
        />
      )}
    </div>
  );
}

function WorldRail({
  entries,
  label,
  activeId,
  onPick,
  onCreate,
  onDelete,
}: {
  entries: WorldEntry[];
  label: string;
  activeId: string | null;
  onPick: (id: string) => void;
  onCreate: () => void;
  onDelete: (id: string) => void;
}): JSX.Element {
  return (
    <aside className="flex w-60 shrink-0 flex-col border-r border-line-subtle bg-surface-subtle">
      <div className="flex items-center justify-between border-b border-line-subtle px-3 py-2 text-xs font-medium uppercase tracking-wide text-ink-faint">
        <span>{label}s</span>
        <button
          type="button"
          onClick={onCreate}
          className="qbtn-ghost inline-flex h-6 items-center gap-1 px-1.5 text-xs"
          title={`New ${label.toLowerCase()}`}
        >
          <Plus className="h-3.5 w-3.5" /> New
        </button>
      </div>
      <div className="flex-1 overflow-y-auto py-1">
        {entries.length === 0 ? (
          <div className="px-3 py-4 text-xs text-ink-faint">
            No {label.toLowerCase()}s yet.
          </div>
        ) : (
          entries.map((e) => (
            <div
              key={e.id}
              className={cn(
                "group flex items-center gap-2 px-3 py-1.5 text-sm",
                e.id === activeId
                  ? "bg-amber-50 text-ink dark:bg-amber-950/30"
                  : "hover:bg-surface-elevated",
              )}
            >
              <button
                type="button"
                onClick={() => onPick(e.id)}
                className="flex min-w-0 flex-1 flex-col items-start text-left"
              >
                <span className="flex w-full items-center gap-1.5">
                  <span className="truncate font-medium">{e.name}</span>
                  {e.ai_suggested && <AISuggestedBadge sourceDocId={e.source_doc_id} />}
                </span>
                {e.description && (
                  <span className="truncate text-[10px] text-ink-faint">
                    {e.description}
                  </span>
                )}
              </button>
              <button
                type="button"
                onClick={() => onDelete(e.id)}
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

function WorldEditor({
  entry,
  onPatch,
}: {
  entry: WorldEntry;
  onPatch: (patch: WorldEntryPatch) => Promise<void>;
}): JSX.Element {
  const [draft, setDraft] = useState<WorldEntry>(entry);
  useEffect(() => setDraft(entry), [entry]);

  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const queuePatch = useCallback(
    (patch: WorldEntryPatch) => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
      debounceRef.current = setTimeout(() => {
        void onPatch(patch);
      }, PATCH_DEBOUNCE_MS);
    },
    [onPatch],
  );

  const update = (key: keyof WorldEntry, value: unknown): void => {
    setDraft((curr) => ({ ...curr, [key]: value }));
    if (key === "aliases") {
      const list = (value as string)
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean);
      queuePatch({ aliases: list });
    } else {
      queuePatch({ [key]: value } as WorldEntryPatch);
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
        <Field label="Type">
          <select
            value={draft.kind}
            onChange={(e) => update("kind", e.target.value as WorldKind)}
            className="qinput w-full"
          >
            {(["location", "faction", "lore"] as WorldKind[]).map((k) => (
              <option key={k} value={k}>
                {WORLD_KIND_LABEL[k]}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Aliases (comma-separated)">
          <input
            type="text"
            value={draft.aliases.join(", ")}
            onChange={(e) => update("aliases", e.target.value)}
            className="qinput w-full"
            placeholder="Alternate names or spellings"
          />
        </Field>
        <Field label="Description">
          <textarea
            value={draft.description}
            onChange={(e) => update("description", e.target.value)}
            rows={10}
            className="qinput w-full font-prose"
            placeholder="What is it? What matters about it? How does it bear on the story?"
          />
        </Field>
        {draft.ai_suggested && (
          <p className="text-xs text-ink-faint">
            ✨ Extracted from canon
            {draft.source_doc_id ? ` (${draft.source_doc_id})` : ""}. Edit freely — your
            changes stick.
          </p>
        )}
      </div>
    </div>
  );
}

function WorldEmptyState({
  kind,
  onCreate,
}: {
  kind: WorldKind;
  onCreate: () => void;
}): JSX.Element {
  const label = WORLD_KIND_LABEL[kind].toLowerCase();
  return (
    <div className="flex flex-1 items-center justify-center p-8">
      <div className="max-w-md text-center text-ink-muted">
        <Globe2 className="mx-auto mb-3 h-8 w-8 text-ink-faint" />
        <p className="text-base">No {label} selected.</p>
        <p className="mt-2 text-sm text-ink-subtle">
          Ingest worldbuilding notes in Canon to auto-populate this, or add one by hand.
        </p>
        <button type="button" onClick={onCreate} className="qbtn-primary mt-4">
          <Plus className="mr-1.5 h-4 w-4" /> New {label}
        </button>
      </div>
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
                className="flex min-w-0 flex-1 flex-col items-start text-left"
              >
                <span className="flex w-full items-center gap-1.5">
                  <span className="truncate font-medium">{c.name}</span>
                  {c.ai_suggested && <AISuggestedBadge sourceDocId={c.source_doc_id} />}
                </span>
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
