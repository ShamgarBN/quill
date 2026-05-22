/**
 * Research view — Phase 4.
 *
 * Manages reference voice pins. Each pin is a passage of prose plus metadata
 * (author, source, weight). Pins drive the project's voice fingerprint, which
 * Phase 5 generation conditions on and Phase 6 drift-detection compares
 * against.
 *
 * The user can also paste a candidate passage and see how far it drifts from
 * their fingerprint — useful for testing your own prose against your refs.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { Plus, Power, PowerOff, Sparkles, Trash2, Wand2 } from "lucide-react";
import { useApp } from "@/stores/app";
import * as ipc from "@/lib/ipc";
import type {
  DriftReport,
  ReferencePin,
  ReferencePinPatch,
  VoiceFingerprint,
} from "@/types";
import { ViewHeader } from "@/routes/Manuscript";
import { cn } from "@/lib/cn";

export function ResearchView(): JSX.Element {
  const project = useApp((s) => s.currentProject);
  const [pins, setPins] = useState<ReferencePin[]>([]);
  const [fp, setFp] = useState<VoiceFingerprint | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  const refresh = useCallback(async () => {
    if (!project) return;
    try {
      const [list, f] = await Promise.all([
        ipc.voicePinsList(project.id),
        ipc.voiceFingerprint(project.id),
      ]);
      setPins(list);
      setFp(f);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [project]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const enabled = pins.filter((p) => p.enabled).length;
  const totalWords = useMemo(
    () =>
      pins.reduce((acc, p) => acc + (p.enabled ? p.passage.split(/\s+/).length : 0), 0),
    [pins],
  );

  if (!project) {
    return (
      <div className="flex h-full flex-col">
        <ViewHeader title="Research" subtitle="Open a project first" />
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <ViewHeader
        title="Research"
        subtitle={`${enabled} active reference${enabled === 1 ? "" : "s"} · ${totalWords.toLocaleString()} words backing the fingerprint`}
        right={
          <button
            type="button"
            className="qbtn-primary inline-flex items-center gap-2"
            onClick={() => setCreating(true)}
          >
            <Plus className="h-4 w-4" /> New pin
          </button>
        }
      />

      <div className="flex min-h-0 flex-1 flex-col gap-6 overflow-y-auto p-6">
        {error && (
          <div className="rounded-md border border-rose-300 bg-rose-50 px-4 py-3 text-sm text-rose-900 dark:border-rose-700/40 dark:bg-rose-900/20 dark:text-rose-200">
            {error}
            <button
              className="ml-3 text-xs underline"
              type="button"
              onClick={() => setError(null)}
            >
              dismiss
            </button>
          </div>
        )}

        {creating && (
          <NewPinCard
            onCancel={() => setCreating(false)}
            onCreate={async (label, passage) => {
              try {
                await ipc.voicePinsCreate(project.id, label, passage);
                setCreating(false);
                await refresh();
              } catch (e) {
                setError(e instanceof Error ? e.message : String(e));
              }
            }}
          />
        )}

        {pins.length === 0 && !creating && <EmptyState />}

        {pins.length > 0 && (
          <section className="flex flex-col gap-3">
            {pins.map((pin) => (
              <PinCard
                key={pin.id}
                pin={pin}
                onUpdate={async (patch) => {
                  try {
                    await ipc.voicePinsUpdate(project.id, pin.id, patch);
                    await refresh();
                  } catch (e) {
                    setError(e instanceof Error ? e.message : String(e));
                  }
                }}
                onDelete={async () => {
                  try {
                    await ipc.voicePinsDelete(project.id, pin.id);
                    await refresh();
                  } catch (e) {
                    setError(e instanceof Error ? e.message : String(e));
                  }
                }}
              />
            ))}
          </section>
        )}

        {fp && fp.passage_count > 0 && (
          <DriftTester projectId={project.id} fingerprint={fp} />
        )}
      </div>
    </div>
  );
}

function EmptyState(): JSX.Element {
  return (
    <div className="qcard flex flex-col items-center gap-3 px-6 py-12 text-center">
      <Sparkles className="h-8 w-8 text-accent" />
      <h3 className="text-base font-semibold text-ink">No references yet</h3>
      <p className="max-w-prose text-sm text-ink-muted">
        Paste 3–5 short passages from your reference shelf — Eragon, Percy Jackson,
        Harry Potter, Wingfeather, your own old work — and we'll build a voice
        fingerprint. Generation conditions on this fingerprint; revision detects drift
        from it.
      </p>
    </div>
  );
}

function NewPinCard({
  onCancel,
  onCreate,
}: {
  onCancel: () => void;
  onCreate: (label: string, passage: string) => Promise<void>;
}): JSX.Element {
  const [label, setLabel] = useState("");
  const [passage, setPassage] = useState("");
  const [busy, setBusy] = useState(false);

  return (
    <div className="qcard p-4">
      <div className="text-sm font-semibold text-ink">New reference pin</div>
      <div className="mt-1 text-xs text-ink-faint">
        Pick a memorable label (e.g. "Eragon ch1 — Saphira hatching") and paste a
        200–800 word passage.
      </div>
      <div className="mt-3 flex flex-col gap-2">
        <input
          type="text"
          className="qinput"
          placeholder="label"
          value={label}
          onChange={(e) => setLabel(e.target.value)}
        />
        <textarea
          className="qinput min-h-[180px] resize-y leading-relaxed"
          placeholder="paste reference passage…"
          value={passage}
          onChange={(e) => setPassage(e.target.value)}
        />
      </div>
      <div className="mt-3 flex justify-end gap-2">
        <button type="button" className="qbtn-ghost" onClick={onCancel}>
          Cancel
        </button>
        <button
          type="button"
          className="qbtn-primary"
          disabled={busy || !label.trim() || !passage.trim()}
          onClick={async () => {
            setBusy(true);
            try {
              await onCreate(label.trim(), passage);
            } finally {
              setBusy(false);
            }
          }}
        >
          {busy ? "Saving…" : "Save pin"}
        </button>
      </div>
    </div>
  );
}

function PinCard({
  pin,
  onUpdate,
  onDelete,
}: {
  pin: ReferencePin;
  onUpdate: (patch: ReferencePinPatch) => Promise<void>;
  onDelete: () => Promise<void>;
}): JSX.Element {
  const [showFull, setShowFull] = useState(false);
  const [draftWeight, setDraftWeight] = useState(pin.weight.toString());
  useEffect(() => setDraftWeight(pin.weight.toString()), [pin.weight]);

  const wordCount = useMemo(
    () => pin.passage.trim().split(/\s+/).length,
    [pin.passage],
  );

  return (
    <div className={cn("qcard p-4", !pin.enabled && "opacity-60")}>
      <div className="flex items-baseline justify-between gap-3">
        <div className="min-w-0">
          <div className="text-sm font-semibold text-ink">{pin.label}</div>
          <div className="mt-0.5 text-xs text-ink-faint">
            {wordCount.toLocaleString()} words
            {pin.author && ` · ${pin.author}`}
            {pin.source && ` · ${pin.source}`}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <label className="inline-flex items-center gap-1.5 text-xs text-ink-muted">
            weight
            <input
              type="number"
              min={0}
              max={10}
              step={0.5}
              className="qinput w-16 text-right tabular-nums"
              value={draftWeight}
              onChange={(e) => setDraftWeight(e.target.value)}
              onBlur={() => {
                const n = Number(draftWeight);
                if (Number.isFinite(n) && n !== pin.weight) {
                  void onUpdate({ weight: n });
                }
              }}
            />
          </label>
          <button
            type="button"
            className="qbtn-ghost"
            title={pin.enabled ? "Disable pin" : "Enable pin"}
            onClick={() => void onUpdate({ enabled: !pin.enabled })}
          >
            {pin.enabled ? (
              <Power className="h-4 w-4 text-emerald-600 dark:text-emerald-400" />
            ) : (
              <PowerOff className="h-4 w-4 text-ink-faint" />
            )}
          </button>
          <button
            type="button"
            className="qbtn-ghost"
            onClick={() => void onDelete()}
            title="Delete pin"
          >
            <Trash2 className="h-4 w-4 text-rose-600 dark:text-rose-400" />
          </button>
        </div>
      </div>

      <button
        type="button"
        className="mt-3 w-full text-left"
        onClick={() => setShowFull((v) => !v)}
      >
        <p
          className={cn(
            "whitespace-pre-wrap text-sm leading-relaxed text-ink-muted",
            !showFull && "line-clamp-3",
          )}
        >
          {pin.passage}
        </p>
        <span className="mt-1 inline-block text-xs text-accent">
          {showFull ? "show less" : "show more"}
        </span>
      </button>
    </div>
  );
}

function DriftTester({
  projectId,
  fingerprint,
}: {
  projectId: string;
  fingerprint: VoiceFingerprint;
}): JSX.Element {
  const [text, setText] = useState("");
  const [drift, setDrift] = useState<DriftReport | null>(null);
  const [busy, setBusy] = useState(false);

  const onTest = useCallback(async () => {
    if (!text.trim()) return;
    setBusy(true);
    try {
      const r = await ipc.voiceDrift(projectId, text, 8);
      setDrift(r);
    } finally {
      setBusy(false);
    }
  }, [projectId, text]);

  return (
    <section>
      <h2 className="mb-2 text-xs font-semibold uppercase tracking-wider text-ink-faint">
        Voice drift tester
      </h2>
      <div className="qcard p-4">
        <div className="text-xs text-ink-muted">
          Fingerprint built from {fingerprint.passage_count} passages ·{" "}
          {fingerprint.total_words.toLocaleString()} words.
        </div>
        <textarea
          className="qinput mt-3 min-h-[160px] resize-y leading-relaxed"
          placeholder="Paste a candidate passage to test drift…"
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
        <div className="mt-3 flex justify-end">
          <button
            type="button"
            className="qbtn-primary inline-flex items-center gap-2"
            disabled={busy || !text.trim()}
            onClick={() => void onTest()}
          >
            <Wand2 className="h-4 w-4" />
            {busy ? "Analyzing…" : "Compare to fingerprint"}
          </button>
        </div>

        {drift && (
          <div className="mt-4 grid grid-cols-1 gap-4 md:grid-cols-2">
            <DriftMeter score={drift.drift_score} cosine={drift.cosine} />
            <div>
              <div className="text-xs font-medium text-ink-muted">
                Top deltas (z-score against fingerprint)
              </div>
              <ul className="mt-2 flex flex-col gap-1.5">
                {drift.top_deltas.map((d, i) => (
                  <li key={i} className="flex items-center gap-2 text-xs">
                    <span className="w-32 truncate text-ink-muted">{d.label}</span>
                    <ZBar z={d.z_score} />
                    <span className="w-12 shrink-0 text-right tabular-nums text-ink-faint">
                      {d.z_score >= 0 ? "+" : ""}
                      {d.z_score.toFixed(2)}
                    </span>
                  </li>
                ))}
              </ul>
            </div>
          </div>
        )}
      </div>
    </section>
  );
}

function DriftMeter({ score, cosine }: { score: number; cosine: number }): JSX.Element {
  const pct = Math.round(score * 100);
  const tone =
    score < 0.15 ? "bg-emerald-500" : score < 0.35 ? "bg-amber-500" : "bg-rose-500";
  return (
    <div>
      <div className="text-xs font-medium text-ink-muted">Overall drift</div>
      <div className="mt-2 flex items-baseline gap-3">
        <span className="text-3xl font-semibold tabular-nums text-ink">{pct}%</span>
        <span className="text-xs text-ink-faint">cosine {cosine.toFixed(3)}</span>
      </div>
      <div className="mt-2 h-2 w-full overflow-hidden rounded-full bg-surface-muted">
        <div
          className={cn("h-full transition-all", tone)}
          style={{ width: `${pct}%` }}
        />
      </div>
      <div className="mt-2 text-xs text-ink-faint">
        Under 15% feels on-voice. Over 35% is a clear shift — investigate.
      </div>
    </div>
  );
}

function ZBar({ z }: { z: number }): JSX.Element {
  const clamped = Math.max(-3, Math.min(3, z));
  const pct = (Math.abs(clamped) / 3) * 50;
  const positive = clamped >= 0;
  return (
    <div className="relative h-1.5 flex-1 rounded-full bg-surface-muted">
      <div className="absolute left-1/2 top-0 h-full w-px bg-line" />
      <div
        className={cn(
          "absolute top-0 h-full",
          positive ? "left-1/2 bg-amber-500" : "right-1/2 bg-sky-500",
        )}
        style={{ width: `${pct}%` }}
      />
    </div>
  );
}
