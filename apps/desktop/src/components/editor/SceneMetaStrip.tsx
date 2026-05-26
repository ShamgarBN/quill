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
import { ChevronDown } from "lucide-react";
import * as ipc from "@/lib/ipc";
import {
  BEAT_META,
  BEAT_ORDER,
  type BeatId,
  type Scene,
  type SceneStatus,
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
}

export function SceneMetaStrip({
  projectId,
  scene,
  onSceneUpdated,
}: Props): JSX.Element {
  // Local edit state for text fields so the parent doesn't re-render the
  // editor on every keystroke.
  const [pov, setPov] = useState(scene.pov ?? "");
  const [setting, setSetting] = useState(scene.setting ?? "");
  const [error, setError] = useState<string | null>(null);

  // Reset local state whenever the user switches scenes.
  useEffect(() => {
    setPov(scene.pov ?? "");
    setSetting(scene.setting ?? "");
    setError(null);
  }, [scene.id, scene.pov, scene.setting]);

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

        <div className="ml-auto text-ink-faint">
          {scene.word_count.toLocaleString()} words
        </div>
      </div>
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
