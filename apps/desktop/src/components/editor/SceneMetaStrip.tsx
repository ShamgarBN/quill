/**
 * SceneMetaStrip — compact row above the editor for the per-scene
 * metadata fields the writer touches often (POV, setting, status, beat).
 *
 * These fields live on the Scene record managed by the structure store.
 * Until now they were only editable from the Beat Sheet view, which forced
 * the user to leave their writing flow to flip a status or change a beat.
 *
 * Saves are best-effort:
 *  - Text inputs (POV, setting) commit on blur, not per keystroke.
 *  - Selects (status, beat) commit immediately.
 *
 * On save, the parent's `onSceneUpdated` callback is called with the
 * fresh Scene so the parent can update its `scenes` array.
 */
import { useEffect, useState } from "react";
import { ChevronDown, FolderSearch, GitBranch } from "lucide-react";
import * as ipc from "@/lib/ipc";
import {
  BEAT_META,
  BEAT_ORDER,
  type BeatId,
  type Scene,
  type SceneStatus,
  type Thread,
} from "@/types";
import { cn } from "@/lib/cn";

const STATUS_OPTIONS: { value: SceneStatus; label: string }[] = [
  { value: "outlined", label: "Outlined" },
  { value: "drafting", label: "Drafting" },
  { value: "drafted", label: "Drafted" },
  { value: "revised", label: "Revised" },
  { value: "locked", label: "Locked" },
];

const STATUS_TONE: Record<SceneStatus, string> = {
  outlined: "text-ink-faint",
  drafting: "text-amber-700 dark:text-amber-300",
  drafted: "text-sky-700 dark:text-sky-300",
  revised: "text-emerald-700 dark:text-emerald-300",
  locked: "text-ink",
};

interface Props {
  projectId: string;
  scene: Scene;
  onSceneUpdated: (scene: Scene) => void;
  /** On-disk path of the scene's .md file, for the reveal-in-Finder button. */
  scenePath?: string;
}

export function SceneMetaStrip({
  projectId,
  scene,
  onSceneUpdated,
  scenePath,
}: Props): JSX.Element {
  // Local edit state for text fields so the parent doesn't re-render the
  // editor on every keystroke.
  const [pov, setPov] = useState(scene.pov ?? "");
  const [setting, setSetting] = useState(scene.setting ?? "");
  const [error, setError] = useState<string | null>(null);
  /** Available threads for the project; loaded once per scene render. */
  const [threads, setThreads] = useState<Thread[]>([]);

  // Reset local state whenever the user switches scenes.
  useEffect(() => {
    setPov(scene.pov ?? "");
    setSetting(scene.setting ?? "");
    setError(null);
  }, [scene.id, scene.pov, scene.setting]);

  // Refresh thread list when the project changes (or scene changes —
  // cheap, and ensures newly-created threads show up immediately).
  useEffect(() => {
    let cancelled = false;
    void ipc
      .brainThreadsList(projectId)
      .then((list) => {
        if (!cancelled) setThreads(list);
      })
      .catch(() => {
        // Non-fatal — thread chips just won't show.
      });
    return () => {
      cancelled = true;
    };
  }, [projectId, scene.id]);

  const persist = async (patch: Parameters<typeof ipc.structureSceneUpdate>[2]) => {
    try {
      const updated = await ipc.structureSceneUpdate(projectId, scene.id, patch);
      onSceneUpdated(updated);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const commitPov = (): void => {
    const next: string | null = pov.trim() === "" ? null : pov.trim();
    if (next === (scene.pov ?? null)) return;
    void persist({ pov: next });
  };

  const commitSetting = (): void => {
    const next: string | null = setting.trim() === "" ? null : setting.trim();
    if (next === (scene.setting ?? null)) return;
    void persist({ setting: next });
  };

  const toggleThread = (id: string): void => {
    const curr = scene.thread_ids ?? [];
    const next = curr.includes(id) ? curr.filter((x) => x !== id) : [...curr, id];
    void persist({ thread_ids: next });
  };

  const linkedThreads = (scene.thread_ids ?? [])
    .map((id) => threads.find((t) => t.id === id))
    .filter((t): t is Thread => t !== undefined);
  const unlinkedThreads = threads.filter(
    (t) =>
      !(scene.thread_ids ?? []).includes(t.id) &&
      (t.status === "open" || t.status === "advancing"),
  );

  return (
    <div className="border-b border-line-subtle bg-surface-subtle px-5 py-2">
      <div className="flex flex-wrap items-center gap-3 text-xs">
        <Field label="POV">
          <input
            type="text"
            value={pov}
            onChange={(e) => setPov(e.target.value)}
            onBlur={commitPov}
            onKeyDown={(e) => {
              if (e.key === "Enter") (e.target as HTMLInputElement).blur();
            }}
            placeholder="e.g. Kaelan, 3rd-limited"
            className="qinput h-7 w-40 px-2 text-xs"
          />
        </Field>

        <Field label="Setting">
          <input
            type="text"
            value={setting}
            onChange={(e) => setSetting(e.target.value)}
            onBlur={commitSetting}
            onKeyDown={(e) => {
              if (e.key === "Enter") (e.target as HTMLInputElement).blur();
            }}
            placeholder="e.g. The Hollow Wastes, dusk"
            className="qinput h-7 w-56 px-2 text-xs"
          />
        </Field>

        <Field label="Status">
          <SelectChip>
            <select
              value={scene.status}
              onChange={(e) => void persist({ status: e.target.value as SceneStatus })}
              className={cn(
                "appearance-none bg-transparent pr-5 text-xs font-medium outline-none",
                STATUS_TONE[scene.status],
              )}
            >
              {STATUS_OPTIONS.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </select>
          </SelectChip>
        </Field>

        <Field label="Beat">
          <SelectChip>
            <select
              value={scene.beat_id ?? ""}
              onChange={(e) =>
                void persist({
                  beat_id: e.target.value === "" ? null : (e.target.value as BeatId),
                })
              }
              className="appearance-none bg-transparent pr-5 text-xs outline-none"
            >
              <option value="">— unassigned —</option>
              {BEAT_ORDER.map((id) => (
                <option key={id} value={id}>
                  {String(BEAT_META[id].order + 1).padStart(2, "0")}.{" "}
                  {BEAT_META[id].label}
                </option>
              ))}
            </select>
          </SelectChip>
        </Field>

        <div className="ml-auto flex items-center gap-2 text-ink-faint">
          <span>{scene.word_count.toLocaleString()} words</span>
          {scenePath && (
            <button
              type="button"
              onClick={() =>
                void ipc.systemRevealPath(scenePath).catch(() => undefined)
              }
              className="rounded p-0.5 hover:bg-surface-elevated hover:text-ink"
              title={`Reveal in Finder · ${scenePath}`}
              aria-label="Reveal scene file in Finder"
            >
              <FolderSearch className="h-3.5 w-3.5" />
            </button>
          )}
        </div>
      </div>
      {(linkedThreads.length > 0 || unlinkedThreads.length > 0) && (
        <div className="mt-1.5 flex flex-wrap items-center gap-1 text-[11px]">
          <GitBranch className="h-3 w-3 text-ink-faint" />
          <span className="text-ink-faint">Threads:</span>
          {linkedThreads.length === 0 && (
            <span className="italic text-ink-faint">none linked</span>
          )}
          {linkedThreads.map((t) => (
            <button
              key={t.id}
              type="button"
              onClick={() => toggleThread(t.id)}
              title="Unlink from this scene"
              className="inline-flex items-center gap-1 rounded-full bg-violet-100 px-2 py-0.5 text-violet-900 hover:bg-violet-200 dark:bg-violet-900/30 dark:text-violet-200 dark:hover:bg-violet-900/50"
            >
              {t.title}
              <span aria-hidden="true">×</span>
            </button>
          ))}
          {unlinkedThreads.length > 0 && (
            <SelectChip>
              <select
                value=""
                onChange={(e) => {
                  if (e.target.value) toggleThread(e.target.value);
                }}
                className="appearance-none bg-transparent pr-5 text-[11px] outline-none"
              >
                <option value="">+ link…</option>
                {unlinkedThreads.map((t) => (
                  <option key={t.id} value={t.id}>
                    {t.title}
                  </option>
                ))}
              </select>
            </SelectChip>
          )}
        </div>
      )}
      {error && <div className="mt-1 text-xs text-rose-600">{error}</div>}
    </div>
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
    <label className="inline-flex items-center gap-1.5">
      <span className="font-medium uppercase tracking-wide text-ink-faint">
        {label}
      </span>
      {children}
    </label>
  );
}

function SelectChip({ children }: { children: React.ReactNode }): JSX.Element {
  return (
    <span className="relative inline-flex h-7 items-center rounded-md border border-line-subtle bg-surface px-2">
      {children}
      <ChevronDown className="pointer-events-none absolute right-1.5 h-3 w-3 text-ink-faint" />
    </span>
  );
}
