/**
 * Beat Sheet view — Phase 3.
 *
 * Interactive 15-beat sheet (Save the Cat). Each beat has:
 * - canonical metadata (label, target %, description)
 * - editable summary
 * - lock toggle (frozen content during generation)
 * - satisfied toggle (user confirmation that the beat is hit)
 *
 * The whole sheet can be frozen (no further edits accepted) when the user
 * commits to the structure. There's also an outline-paste import flow.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { Lock, Snowflake, Sparkles, Unlock } from "lucide-react";
import { useApp } from "@/stores/app";
import * as ipc from "@/lib/ipc";
import {
  BEAT_META,
  BEAT_ORDER,
  type Beat,
  type BeatSheet,
  type ImportPreview,
} from "@/types";
import { ViewHeader } from "@/routes/Manuscript";
import { cn } from "@/lib/cn";
import { errToString } from "@/lib/err";

export function BeatsView(): JSX.Element {
  const project = useApp((s) => s.currentProject);
  const [sheet, setSheet] = useState<BeatSheet | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [importOpen, setImportOpen] = useState(false);

  const refresh = useCallback(async () => {
    if (!project) return;
    try {
      const s = await ipc.structureBeatSheetGet(project.id);
      setSheet(s);
    } catch (e) {
      setError(errToString(e));
    }
  }, [project]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  if (!project) {
    return (
      <div className="flex h-full flex-col">
        <ViewHeader title="Beat Sheet" subtitle="Open a project first" />
      </div>
    );
  }

  if (!sheet) {
    return (
      <div className="flex h-full flex-col">
        <ViewHeader title="Beat Sheet" subtitle="Loading…" />
      </div>
    );
  }

  const satisfied = sheet.beats.filter((b) => b.satisfied).length;

  return (
    <div className="flex h-full flex-col">
      <ViewHeader
        title="Beat Sheet"
        subtitle={`${satisfied} / 15 satisfied · target ${sheet.target_word_count.toLocaleString()} words`}
        right={
          <div className="flex items-center gap-2">
            <button
              type="button"
              className="qbtn-outline inline-flex items-center gap-2"
              onClick={() => setImportOpen(true)}
            >
              <Sparkles className="h-4 w-4" /> Import outline
            </button>
            <FreezeToggle
              frozen={sheet.frozen}
              onToggle={async () => {
                try {
                  const s = await ipc.structureBeatSheetSetFrozen(
                    project.id,
                    !sheet.frozen,
                  );
                  setSheet(s);
                } catch (e) {
                  setError(errToString(e));
                }
              }}
            />
          </div>
        }
      />
      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-6">
        {error && (
          <div className="rounded-md border border-rose-300 bg-rose-50 px-4 py-3 text-sm text-rose-900 dark:border-rose-700/40 dark:bg-rose-900/20 dark:text-rose-200">
            {error}
            <button
              type="button"
              className="ml-3 text-xs underline"
              onClick={() => setError(null)}
            >
              dismiss
            </button>
          </div>
        )}

        <TargetWordRow
          target={sheet.target_word_count}
          frozen={sheet.frozen}
          onChange={async (next) => {
            try {
              const s = await ipc.structureBeatSheetSetTarget(project.id, next);
              setSheet(s);
            } catch (e) {
              setError(errToString(e));
            }
          }}
        />

        <div className="flex flex-col gap-2">
          {BEAT_ORDER.map((id) => {
            const beat = sheet.beats.find((b) => b.id === id);
            if (!beat) return null;
            return (
              <BeatRow
                key={id}
                beat={beat}
                target={sheet.target_word_count}
                frozen={sheet.frozen}
                onSummary={async (summary) => {
                  try {
                    const s = await ipc.structureBeatUpdate(project.id, id, {
                      summary,
                    });
                    setSheet(s);
                  } catch (e) {
                    setError(errToString(e));
                  }
                }}
                onSatisfied={async (satisfied) => {
                  try {
                    const s = await ipc.structureBeatUpdate(project.id, id, {
                      satisfied,
                    });
                    setSheet(s);
                  } catch (e) {
                    setError(errToString(e));
                  }
                }}
                onLocked={async (locked) => {
                  try {
                    const s = await ipc.structureBeatUpdate(project.id, id, {
                      locked,
                    });
                    setSheet(s);
                  } catch (e) {
                    setError(errToString(e));
                  }
                }}
              />
            );
          })}
        </div>
      </div>

      {importOpen && (
        <OutlineImportModal
          projectId={project.id}
          onClose={() => setImportOpen(false)}
          onApplied={async () => {
            setImportOpen(false);
            await refresh();
          }}
        />
      )}
    </div>
  );
}

function TargetWordRow({
  target,
  frozen,
  onChange,
}: {
  target: number;
  frozen: boolean;
  onChange: (n: number) => Promise<void>;
}): JSX.Element {
  const [draft, setDraft] = useState(target.toString());
  useEffect(() => setDraft(target.toString()), [target]);

  return (
    <div className="qcard flex items-center gap-4 px-4 py-3">
      <div className="flex-1">
        <div className="text-sm font-medium text-ink">Target manuscript length</div>
        <div className="text-xs text-ink-faint">
          Drives the % position of each beat. YA fantasy typically 80–100k.
        </div>
      </div>
      <input
        type="number"
        className="qinput w-32 text-right tabular-nums"
        value={draft}
        min={20000}
        max={250000}
        step={1000}
        disabled={frozen}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={() => {
          const n = Number(draft);
          if (Number.isFinite(n) && n !== target) void onChange(n);
        }}
      />
      <span className="text-xs text-ink-faint">words</span>
    </div>
  );
}

function FreezeToggle({
  frozen,
  onToggle,
}: {
  frozen: boolean;
  onToggle: () => void;
}): JSX.Element {
  return (
    <button
      type="button"
      className={cn(
        "qbtn inline-flex items-center gap-2",
        frozen ? "qbtn-primary" : "qbtn-outline",
      )}
      onClick={onToggle}
      title={frozen ? "Unfreeze sheet" : "Freeze sheet (lock all beats)"}
    >
      <Snowflake className="h-4 w-4" />
      {frozen ? "Frozen" : "Freeze"}
    </button>
  );
}

function BeatRow({
  beat,
  target,
  frozen,
  onSummary,
  onSatisfied,
  onLocked,
}: {
  beat: Beat;
  target: number;
  frozen: boolean;
  onSummary: (s: string) => Promise<void>;
  onSatisfied: (v: boolean) => Promise<void>;
  onLocked: (v: boolean) => Promise<void>;
}): JSX.Element {
  const meta = BEAT_META[beat.id];
  const targetWord = useMemo(() => {
    const pct = beat.override_pct ?? meta.targetPct;
    return Math.round(pct * target);
  }, [beat.override_pct, meta.targetPct, target]);

  const [draft, setDraft] = useState(beat.summary);
  useEffect(() => setDraft(beat.summary), [beat.summary]);

  const dirty = draft !== beat.summary;

  return (
    <div
      className={cn(
        "qcard p-4 transition-colors",
        beat.satisfied && "border-emerald-300/70 dark:border-emerald-700/50",
        beat.locked && "ring-1 ring-amber-300/70 dark:ring-amber-700/50",
      )}
    >
      <div className="flex items-baseline justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-baseline gap-2">
            <span className="text-xs font-mono text-ink-faint tabular-nums">
              #{(meta.order + 1).toString().padStart(2, "0")}
            </span>
            <h3 className="text-sm font-semibold text-ink">{meta.label}</h3>
            <span className="text-xs text-ink-faint">
              ~{Math.round(meta.targetPct * 100)}% · ≈{targetWord.toLocaleString()} w
            </span>
          </div>
          <p className="mt-1 text-xs text-ink-muted">{meta.description}</p>
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          <IconToggle
            on={beat.satisfied}
            disabled={frozen}
            onClick={() => void onSatisfied(!beat.satisfied)}
            title={beat.satisfied ? "Mark unsatisfied" : "Mark satisfied"}
            label={beat.satisfied ? "✓ Done" : "Mark done"}
            tone="emerald"
          />
          <IconToggle
            on={beat.locked}
            disabled={frozen}
            onClick={() => void onLocked(!beat.locked)}
            title={beat.locked ? "Unlock beat" : "Lock beat"}
            label={beat.locked ? "Locked" : "Lock"}
            icon={beat.locked ? Lock : Unlock}
            tone="amber"
          />
        </div>
      </div>
      <textarea
        className="qinput mt-3 min-h-[64px] resize-y leading-relaxed"
        placeholder={`What happens at ${meta.label.toLowerCase()}?`}
        value={draft}
        disabled={frozen || beat.locked}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={() => {
          if (dirty) void onSummary(draft);
        }}
      />
      {dirty && (
        <div className="mt-1 text-xs text-ink-faint">
          unsaved · click out of the box to save
        </div>
      )}
    </div>
  );
}

function IconToggle({
  on,
  disabled,
  onClick,
  title,
  label,
  icon: IconCmp,
  tone,
}: {
  on: boolean;
  disabled: boolean;
  onClick: () => void;
  title: string;
  label: string;
  icon?: typeof Lock;
  tone: "emerald" | "amber";
}): JSX.Element {
  const palettes = {
    emerald: {
      on: "bg-emerald-100 text-emerald-800 dark:bg-emerald-900/30 dark:text-emerald-300",
      off: "text-ink-muted hover:bg-surface-muted",
    },
    amber: {
      on: "bg-amber-100 text-amber-800 dark:bg-amber-900/30 dark:text-amber-300",
      off: "text-ink-muted hover:bg-surface-muted",
    },
  };
  const styles = palettes[tone];
  return (
    <button
      type="button"
      className={cn(
        "rounded-md px-2 py-1 text-xs font-medium transition-colors",
        on ? styles.on : styles.off,
        disabled && "cursor-not-allowed opacity-50",
      )}
      title={title}
      disabled={disabled}
      onClick={onClick}
    >
      <span className="inline-flex items-center gap-1">
        {IconCmp && <IconCmp className="h-3 w-3" />}
        {label}
      </span>
    </button>
  );
}

function OutlineImportModal({
  projectId,
  onClose,
  onApplied,
}: {
  projectId: string;
  onClose: () => void;
  onApplied: () => void;
}): JSX.Element {
  const [text, setText] = useState("");
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const onPreview = async (): Promise<void> => {
    setBusy(true);
    setErr(null);
    try {
      const p = await ipc.structureOutlinePreview(text);
      setPreview(p);
    } catch (e) {
      setErr(errToString(e));
    } finally {
      setBusy(false);
    }
  };

  const onApply = async (): Promise<void> => {
    setBusy(true);
    setErr(null);
    try {
      await ipc.structureOutlineApply(projectId, text);
      onApplied();
    } catch (e) {
      setErr(errToString(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/50 p-8">
      <div className="qcard flex max-h-[80vh] w-full max-w-3xl flex-col overflow-hidden">
        <div className="border-b border-line-subtle px-5 py-3">
          <div className="text-sm font-semibold text-ink">Import outline</div>
          <div className="mt-0.5 text-xs text-ink-faint">
            Paste a draft outline. We'll detect Save the Cat beats by their labels and
            route the surrounding text into the right slot.
          </div>
        </div>
        <div className="flex flex-1 gap-4 overflow-hidden p-5">
          <div className="flex flex-1 flex-col">
            <label className="text-xs font-medium text-ink-muted">Outline</label>
            <textarea
              className="qinput mt-2 flex-1 resize-none font-mono text-xs leading-relaxed"
              placeholder={"# Catalyst\n\nA letter arrives at dawn…"}
              value={text}
              onChange={(e) => setText(e.target.value)}
            />
          </div>
          <div className="flex flex-1 flex-col">
            <label className="text-xs font-medium text-ink-muted">Preview</label>
            <div className="mt-2 flex-1 overflow-y-auto rounded-md border border-line-subtle bg-surface-subtle p-3 text-xs">
              {preview ? (
                <>
                  {preview.matched.length === 0 ? (
                    <div className="text-ink-faint">No beats matched.</div>
                  ) : (
                    <ul className="flex flex-col gap-2">
                      {preview.matched.map((m) => (
                        <li key={m.id}>
                          <div className="font-semibold text-ink">{m.label}</div>
                          <div className="mt-0.5 whitespace-pre-wrap text-ink-muted">
                            {m.summary || "(empty)"}
                          </div>
                        </li>
                      ))}
                    </ul>
                  )}
                  {preview.unmatched.length > 0 && (
                    <div className="mt-3 border-t border-line-subtle pt-2">
                      <div className="font-medium text-ink-muted">Leftovers</div>
                      <pre className="mt-1 whitespace-pre-wrap text-ink-faint">
                        {preview.unmatched.join("\n")}
                      </pre>
                    </div>
                  )}
                </>
              ) : (
                <div className="text-ink-faint">
                  Click "Preview" to dry-run the import.
                </div>
              )}
            </div>
          </div>
        </div>
        {err && (
          <div className="border-t border-line-subtle bg-rose-50 px-5 py-2 text-xs text-rose-900 dark:bg-rose-900/20 dark:text-rose-200">
            {err}
          </div>
        )}
        <div className="flex items-center justify-end gap-2 border-t border-line-subtle px-5 py-3">
          <button type="button" className="qbtn-ghost" onClick={onClose}>
            Cancel
          </button>
          <button
            type="button"
            className="qbtn-outline"
            onClick={() => void onPreview()}
            disabled={busy || !text.trim()}
          >
            {busy ? "…" : "Preview"}
          </button>
          <button
            type="button"
            className="qbtn-primary"
            onClick={() => void onApply()}
            disabled={busy || !text.trim() || !preview || preview.matched.length === 0}
          >
            Apply
          </button>
        </div>
      </div>
    </div>
  );
}
