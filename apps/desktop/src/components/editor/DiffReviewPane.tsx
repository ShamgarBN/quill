/**
 * DiffReviewPane — Phase 6.3 inline track-changes UI.
 *
 * Renders a sentence-level diff between the scene's current text and an
 * AI-generated candidate. Each non-equal chunk gets per-chunk Accept /
 * Reject controls; pending chunks (the default) are skipped on Apply.
 *
 * State machine for each non-equal chunk: pending → accepted → rejected,
 * freely toggled. The committed text is only written to the scene when
 * the user hits Apply (which calls `onApply(finalText)`). Cancel discards
 * everything.
 *
 * Whitespace handling is delegated to `applyDecisions`; this component is
 * presentation only.
 */
import { useMemo, useState } from "react";
import { Check, RotateCcw, Sparkles, X } from "lucide-react";
import { cn } from "@/lib/cn";
import {
  type Decision,
  type DiffChunk,
  applyDecisions,
  diffSentences,
} from "@/lib/sentence_diff";

export type ChunkState = "pending" | "accepted" | "rejected";

interface Props {
  original: string;
  candidate: string;
  onApply: (finalText: string) => void;
  onCancel: () => void;
}

export function DiffReviewPane({
  original,
  candidate,
  onApply,
  onCancel,
}: Props): JSX.Element {
  const chunks = useMemo(
    () => diffSentences(original, candidate),
    [original, candidate],
  );

  // States array — one per chunk. Equal chunks are always "accepted"
  // (they're the user's existing prose). Non-equal start "pending".
  const [states, setStates] = useState<ChunkState[]>(() =>
    chunks.map((c) => (c.op === "equal" ? "accepted" : "pending")),
  );

  const counts = useMemo(() => {
    let pending = 0;
    let accepted = 0;
    let rejected = 0;
    let nonEqual = 0;
    chunks.forEach((c, i) => {
      if (c.op === "equal") return;
      nonEqual += 1;
      const s = states[i];
      if (s === "accepted") accepted += 1;
      else if (s === "rejected") rejected += 1;
      else pending += 1;
    });
    return { pending, accepted, rejected, nonEqual };
  }, [chunks, states]);

  const setOne = (idx: number, state: ChunkState): void => {
    setStates((prev) => {
      const next = prev.slice();
      next[idx] = state;
      return next;
    });
  };

  const setAll = (state: ChunkState): void => {
    setStates((prev) => prev.map((s, i) => (chunks[i]?.op === "equal" ? s : state)));
  };

  const reset = (): void => {
    setStates(chunks.map((c) => (c.op === "equal" ? "accepted" : "pending")));
  };

  const apply = (): void => {
    // Pending = treat as rejected (no change). This is the safe default
    // when the user hits Apply without explicitly deciding everything.
    const decisions: Decision[] = states.map((s) =>
      s === "accepted" ? "accepted" : "rejected",
    );
    const finalText = applyDecisions(chunks, decisions);
    onApply(finalText);
  };

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center justify-between border-b border-line-subtle bg-surface-subtle px-5 py-2.5">
        <div className="flex items-center gap-2 text-sm">
          <Sparkles className="h-4 w-4 text-amber-600 dark:text-amber-400" />
          <span className="font-medium text-ink">Reviewing AI changes</span>
          <span className="text-xs text-ink-faint">
            {counts.accepted}/{counts.nonEqual} accepted
            {counts.pending > 0 && (
              <>
                {" · "}
                <span className="text-amber-700 dark:text-amber-300">
                  {counts.pending} pending
                </span>
              </>
            )}
          </span>
        </div>
        <div className="flex items-center gap-1.5">
          <button
            type="button"
            onClick={() => setAll("accepted")}
            className="qbtn-ghost h-7 px-2 text-xs"
            title="Accept every change"
          >
            <Check className="mr-1 h-3.5 w-3.5" /> Accept all
          </button>
          <button
            type="button"
            onClick={() => setAll("rejected")}
            className="qbtn-ghost h-7 px-2 text-xs"
            title="Reject every change"
          >
            <X className="mr-1 h-3.5 w-3.5" /> Reject all
          </button>
          <button
            type="button"
            onClick={reset}
            className="qbtn-ghost h-7 px-2 text-xs"
            title="Mark all changes pending again"
          >
            <RotateCcw className="mr-1 h-3.5 w-3.5" /> Reset
          </button>
          <div className="mx-1 h-5 w-px bg-line-subtle" />
          <button
            type="button"
            onClick={onCancel}
            className="qbtn-secondary h-7 px-3 text-xs"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={apply}
            className="qbtn-primary h-7 px-3 text-xs"
          >
            Apply
          </button>
        </div>
      </header>

      <div className="flex-1 overflow-y-auto">
        <div className="mx-auto max-w-3xl px-8 py-6">
          <div className="prose-pane whitespace-pre-wrap font-prose text-ink">
            {chunks.map((c, i) => (
              <ChunkView
                key={i}
                chunk={c}
                state={states[i] ?? "pending"}
                onAccept={() => setOne(i, "accepted")}
                onReject={() => setOne(i, "rejected")}
              />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

function ChunkView({
  chunk,
  state,
  onAccept,
  onReject,
}: {
  chunk: DiffChunk;
  state: ChunkState;
  onAccept: () => void;
  onReject: () => void;
}): JSX.Element {
  if (chunk.op === "equal") {
    return <span>{chunk.original.map((s) => s.text + s.trailing).join("")}</span>;
  }

  const originalText = chunk.original.map((s) => s.text + s.trailing).join("");
  const candidateText = chunk.candidate.map((s) => s.text + s.trailing).join("");

  return (
    <span
      className={cn(
        "group relative inline align-baseline",
        state === "pending" &&
          "rounded-sm bg-amber-50 ring-1 ring-amber-300/60 dark:bg-amber-900/15 dark:ring-amber-600/40",
      )}
    >
      {/* Removed/old side */}
      {chunk.op !== "insert" && (
        <span
          className={cn(
            "transition-colors",
            state === "accepted"
              ? "text-red-700/60 line-through decoration-red-700/40 dark:text-red-300/50 dark:decoration-red-300/40"
              : state === "rejected"
                ? "text-ink"
                : "text-red-700 line-through decoration-red-600 dark:text-red-300 dark:decoration-red-400",
          )}
        >
          {originalText}
        </span>
      )}

      {/* Added/new side */}
      {chunk.op !== "delete" && (
        <span
          className={cn(
            "transition-colors",
            state === "accepted"
              ? "rounded-sm bg-emerald-100/70 px-0.5 text-emerald-900 dark:bg-emerald-900/30 dark:text-emerald-100"
              : state === "rejected"
                ? "hidden"
                : "rounded-sm bg-emerald-50 px-0.5 text-emerald-800 underline decoration-emerald-500/60 underline-offset-2 dark:bg-emerald-900/20 dark:text-emerald-200",
          )}
        >
          {candidateText}
        </span>
      )}

      {/* Inline decision controls — appear next to pending chunks always, and
          on hover for decided chunks so the user can flip back. */}
      <ChunkControls state={state} onAccept={onAccept} onReject={onReject} />
    </span>
  );
}

function ChunkControls({
  state,
  onAccept,
  onReject,
}: {
  state: ChunkState;
  onAccept: () => void;
  onReject: () => void;
}): JSX.Element {
  const baseBtn =
    "inline-flex h-5 w-5 items-center justify-center rounded border border-line-subtle bg-surface text-xs shadow-sm transition-colors";
  return (
    <span
      className={cn(
        "ml-1 inline-flex select-none gap-0.5 align-middle",
        state === "pending" ? "opacity-100" : "opacity-0 group-hover:opacity-100",
      )}
    >
      <button
        type="button"
        onClick={onAccept}
        className={cn(
          baseBtn,
          state === "accepted"
            ? "border-emerald-500 bg-emerald-500 text-white"
            : "text-emerald-700 hover:bg-emerald-50 dark:text-emerald-300 dark:hover:bg-emerald-900/30",
        )}
        title={state === "accepted" ? "Currently accepted" : "Accept this change"}
        aria-label="Accept change"
      >
        <Check className="h-3 w-3" />
      </button>
      <button
        type="button"
        onClick={onReject}
        className={cn(
          baseBtn,
          state === "rejected"
            ? "border-rose-500 bg-rose-500 text-white"
            : "text-rose-700 hover:bg-rose-50 dark:text-rose-300 dark:hover:bg-rose-900/30",
        )}
        title={state === "rejected" ? "Currently rejected" : "Reject this change"}
        aria-label="Reject change"
      >
        <X className="h-3 w-3" />
      </button>
    </span>
  );
}
