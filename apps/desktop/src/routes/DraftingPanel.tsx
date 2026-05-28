/**
 * Drafting panel — Phase 6.
 *
 * Side-by-side AI suggestion panel that opens beside the Manuscript editor.
 * The user picks an operation (continue, rewrite, critique), types an
 * instruction, and the panel calls the configured chat provider.
 *
 * Key disciplines that mirror the Rust orchestrator:
 *
 * 1. **Drift gate.** If the current scene's voice drift is high, the
 *    backend refuses Continue/Rewrite calls unless the user explicitly
 *    overrides. This panel surfaces the gate status as a banner before
 *    the call is even made (via `drafting_preview`) so the user knows
 *    *before* they hit send.
 *
 * 2. **What gets sent.** Settings has `show_what_gets_sent` enabled by
 *    default. When on, every send first shows the assembled messages
 *    and asks for explicit confirmation. When off, the panel sends
 *    immediately.
 *
 * 3. **Accept/reject.** The user controls what enters the manuscript:
 *    * Replace selection (Rewrite mode)
 *    * Append to scene (Continue mode)
 *    * Discard
 *    There is no "auto-apply" path. The model never writes to disk; the
 *    user does, by clicking a button.
 *
 * 4. **Critique mode.** Returns Markdown commentary that the user reads
 *    and integrates by hand. There's no "accept" because there's no
 *    prose to insert — only an explicit "copy" path if needed later.
 */
import { useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  Eye,
  EyeOff,
  Loader2,
  RefreshCw,
  Send,
  Shield,
  X,
} from "lucide-react";
import * as ipc from "@/lib/ipc";
import { useApp } from "@/stores/app";
import type {
  ChatMessage,
  DraftOperation,
  DraftPreview,
  DraftRequest,
  DraftSuggestion,
} from "@/types";
import { cn } from "@/lib/cn";

const OPERATIONS: { id: DraftOperation; label: string; hint: string }[] = [
  {
    id: "continue",
    label: "Continue",
    hint: "Pick up the scene from where it ends and push toward the active beat.",
  },
  {
    id: "rewrite",
    label: "Rewrite",
    hint: "Replace the selected passage with a tighter version in your voice.",
  },
  {
    id: "critique",
    label: "Critique",
    hint: "Get notes on the selected passage — voice, pacing, continuity.",
  },
];

interface Props {
  /** Currently selected text in the editor, if any. */
  selection: string;
  /** Active scene id; null disables the panel. */
  sceneId: string | null;
  /** Hand the suggestion off to the editor, which composes the candidate
   *  full-scene text and switches into the inline diff review UI. */
  onReviewChanges: (suggestion: string, operation: DraftOperation) => void;
  /** Close the panel entirely. */
  onClose: () => void;
}

type SendState =
  | { kind: "idle" }
  | { kind: "previewing" }
  | { kind: "preview"; preview: DraftPreview }
  | { kind: "sending"; preview: DraftPreview | null }
  | { kind: "result"; suggestion: DraftSuggestion }
  | { kind: "error"; message: string };

export function DraftingPanel({
  selection,
  sceneId,
  onReviewChanges,
  onClose,
}: Props): JSX.Element {
  const project = useApp((s) => s.currentProject);
  const settings = useApp((s) => s.settings);

  const [operation, setOperation] = useState<DraftOperation>("continue");
  const [instruction, setInstruction] = useState("");
  const [overrideDrift, setOverrideDrift] = useState(false);
  const [send, setSend] = useState<SendState>({ kind: "idle" });

  const showWhatGetsSent = settings?.show_what_gets_sent ?? true;
  const provider = settings?.chat_provider ?? "mock";
  const isMock = provider === "mock";

  // Operations that require a selection to be meaningful.
  const requiresSelection = operation !== "continue";
  const hasSelection = selection.trim().length > 0;
  const blockedReason: string | null = useMemo(() => {
    if (!project || !sceneId) return "Open a scene to draft.";
    if (requiresSelection && !hasSelection)
      return "Select a passage in the editor first.";
    return null;
  }, [project, sceneId, requiresSelection, hasSelection]);

  // Reset the result when the user changes operation/selection so a
  // stale suggestion never gets accidentally accepted.
  useEffect(() => {
    setSend({ kind: "idle" });
  }, [operation, selection]);

  const buildRequest = (override = overrideDrift): DraftRequest | null => {
    if (!project || !sceneId) return null;
    return {
      project_id: project.id,
      scene_id: sceneId,
      operation,
      instruction,
      selection: requiresSelection ? selection : null,
      top_k_canon: 5,
      max_voice_anchors: 3,
      override_drift_gate: override,
    };
  };

  const onPreview = async (): Promise<void> => {
    const req = buildRequest();
    if (!req) return;
    setSend({ kind: "previewing" });
    try {
      const preview = await ipc.draftingPreview(req);
      setSend({ kind: "preview", preview });
    } catch (e) {
      setSend({ kind: "error", message: messageOf(e) });
    }
  };

  const onSendDirect = async (override = overrideDrift): Promise<void> => {
    const req = buildRequest(override);
    if (!req) return;
    setSend((curr) => ({
      kind: "sending",
      preview: curr.kind === "preview" ? curr.preview : null,
    }));
    try {
      const suggestion = await ipc.draftingInvoke(req);
      setSend({ kind: "result", suggestion });
    } catch (e) {
      setSend({ kind: "error", message: messageOf(e) });
    }
  };

  // The big primary button decides whether to preview-first or send-direct
  // based on the user's `show_what_gets_sent` setting.
  const onPrimary = async (): Promise<void> => {
    if (showWhatGetsSent) {
      await onPreview();
    } else {
      await onSendDirect();
    }
  };

  const onConfirmSend = async (): Promise<void> => {
    if (send.kind === "preview" && send.preview.drift_blocks_send && !overrideDrift) {
      // The user must consent to overriding the drift gate before send.
      return;
    }
    await onSendDirect();
  };

  const onAccept = (): void => {
    if (send.kind !== "result") return;
    // Critique returns prose-style commentary, not a replacement — the user
    // reads it in the panel and integrates by hand.
    if (operation === "critique") return;
    onReviewChanges(send.suggestion.content, operation);
    setSend({ kind: "idle" });
  };

  return (
    <aside className="flex w-[420px] shrink-0 flex-col border-l border-line-subtle bg-surface-subtle">
      <header className="flex items-center justify-between border-b border-line-subtle px-4 py-3">
        <div>
          <h2 className="text-sm font-semibold text-ink">Draft with AI</h2>
          <p className="mt-0.5 text-xs text-ink-faint">
            {isMock ? (
              <span className="text-amber-700 dark:text-amber-300">
                Mock provider — set a real chat provider in Settings.
              </span>
            ) : (
              <>
                Provider: <span className="font-medium">{provider}</span>
              </>
            )}
          </p>
        </div>
        <button
          type="button"
          onClick={onClose}
          className="qbtn-ghost h-7 w-7 p-0"
          title="Close drafting panel"
          aria-label="Close drafting panel"
        >
          <X className="h-4 w-4" />
        </button>
      </header>

      <div className="flex flex-1 flex-col overflow-y-auto">
        <section className="border-b border-line-subtle px-4 py-3">
          <div className="flex flex-col gap-2">
            {OPERATIONS.map((op) => (
              <button
                type="button"
                key={op.id}
                onClick={() => setOperation(op.id)}
                className={cn(
                  "flex flex-col items-start gap-0.5 rounded-md border px-3 py-2 text-left text-sm transition-colors",
                  operation === op.id
                    ? "border-accent bg-accent-subtle text-ink"
                    : "border-line-subtle bg-surface text-ink-muted hover:bg-surface-elevated",
                )}
              >
                <span className="font-medium">{op.label}</span>
                <span className="text-xs text-ink-faint">{op.hint}</span>
              </button>
            ))}
          </div>
        </section>

        <section className="border-b border-line-subtle px-4 py-3">
          <label className="block text-xs font-medium uppercase tracking-wide text-ink-faint">
            Instruction
          </label>
          <textarea
            value={instruction}
            onChange={(e) => setInstruction(e.target.value)}
            rows={3}
            placeholder={instructionPlaceholder(operation)}
            className="mt-1.5 w-full resize-y rounded-md border border-line-subtle bg-surface px-2.5 py-1.5 text-sm text-ink outline-none focus:border-accent"
          />
          {requiresSelection && (
            <p className="mt-1.5 text-xs text-ink-faint">
              Selection ({selection.trim().length.toLocaleString()} chars):{" "}
              {hasSelection ? (
                <span className="font-medium text-ink-muted">
                  {truncate(selection.trim(), 80)}
                </span>
              ) : (
                <span className="text-amber-700 dark:text-amber-300">
                  none — select a passage in the editor
                </span>
              )}
            </p>
          )}
        </section>

        {blockedReason && (
          <section className="px-4 py-3 text-xs text-amber-800 dark:text-amber-200">
            {blockedReason}
          </section>
        )}

        {send.kind === "preview" && (
          <PreviewBlock
            preview={send.preview}
            overrideDrift={overrideDrift}
            onToggleOverride={setOverrideDrift}
            onSend={onConfirmSend}
            onCancel={() => setSend({ kind: "idle" })}
          />
        )}

        {send.kind === "sending" && (
          <section className="border-b border-line-subtle bg-surface px-4 py-3 text-sm text-ink-muted">
            <Loader2 className="mr-2 inline h-4 w-4 animate-spin" />
            Drafting…
          </section>
        )}

        {send.kind === "error" && (
          <section className="border-b border-line-subtle bg-red-50 px-4 py-3 text-sm text-red-900 dark:bg-red-950/40 dark:text-red-200">
            <div className="flex items-start gap-2">
              <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
              <div className="min-w-0">
                <div className="font-medium">Draft failed</div>
                <div className="mt-1 break-words text-xs">{send.message}</div>
              </div>
            </div>
            <button
              type="button"
              onClick={() => setSend({ kind: "idle" })}
              className="qbtn-ghost mt-2 h-7 px-2 text-xs"
            >
              Try again
            </button>
          </section>
        )}

        {send.kind === "result" && (
          <ResultBlock
            suggestion={send.suggestion}
            operation={operation}
            onAccept={onAccept}
            onDiscard={() => setSend({ kind: "idle" })}
            onRetry={() => void onPrimary()}
          />
        )}
      </div>

      <footer className="border-t border-line-subtle bg-surface px-4 py-3">
        <button
          type="button"
          disabled={
            blockedReason !== null ||
            send.kind === "previewing" ||
            send.kind === "sending"
          }
          onClick={() => void onPrimary()}
          className="qbtn-primary w-full disabled:cursor-not-allowed disabled:opacity-50"
        >
          {send.kind === "previewing" || send.kind === "sending" ? (
            <Loader2 className="mr-2 h-4 w-4 animate-spin" />
          ) : showWhatGetsSent ? (
            <Eye className="mr-2 h-4 w-4" />
          ) : (
            <Send className="mr-2 h-4 w-4" />
          )}
          {showWhatGetsSent ? "Preview what gets sent" : "Send"}
        </button>
        <p className="mt-2 text-center text-[11px] text-ink-faint">
          {showWhatGetsSent ? (
            <>
              <EyeOff className="mr-1 inline h-3 w-3" />
              Disable previews in Settings → Privacy
            </>
          ) : (
            <>
              <Eye className="mr-1 inline h-3 w-3" />
              Enable previews in Settings → Privacy
            </>
          )}
        </p>
      </footer>
    </aside>
  );
}

// ---------- Subcomponents ----------

function PreviewBlock({
  preview,
  overrideDrift,
  onToggleOverride,
  onSend,
  onCancel,
}: {
  preview: DraftPreview;
  overrideDrift: boolean;
  onToggleOverride: (v: boolean) => void;
  onSend: () => void;
  onCancel: () => void;
}): JSX.Element {
  return (
    <section className="border-b border-line-subtle bg-surface px-4 py-3">
      <div className="mb-2 flex items-center justify-between">
        <h3 className="text-xs font-semibold uppercase tracking-wide text-ink-faint">
          What gets sent
        </h3>
        <span className="text-xs text-ink-faint">
          {preview.canon_chunk_count} canon · {preview.voice_anchor_count} voice
        </span>
      </div>

      {(preview.pov_character_name ||
        preview.setting_canon_count > 0 ||
        preview.idea_count > 0) && (
        <div className="mb-2 flex flex-wrap gap-1.5 text-[11px]">
          {preview.pov_character_name && (
            <span
              className="inline-flex items-center gap-1 rounded-full bg-sky-100 px-2 py-0.5 text-sky-800 dark:bg-sky-900/30 dark:text-sky-200"
              title="Auto-injected from the Character Bible based on the scene's POV"
            >
              POV: {preview.pov_character_name}
            </span>
          )}
          {preview.setting_canon_count > 0 && (
            <span
              className="inline-flex items-center gap-1 rounded-full bg-emerald-100 px-2 py-0.5 text-emerald-800 dark:bg-emerald-900/30 dark:text-emerald-200"
              title="Location/Cosmology canon chunks matched against the scene's setting field"
            >
              Setting match: {preview.setting_canon_count}
            </span>
          )}
          {preview.idea_count > 0 && (
            <span
              className="inline-flex items-center gap-1 rounded-full bg-amber-100 px-2 py-0.5 text-amber-900 dark:bg-amber-900/30 dark:text-amber-200"
              title="Idea Park entries tagged for this beat / scene / POV"
            >
              Ideas: {preview.idea_count}
            </span>
          )}
          {preview.thread_count > 0 && (
            <span
              className="inline-flex items-center gap-1 rounded-full bg-violet-100 px-2 py-0.5 text-violet-900 dark:bg-violet-900/30 dark:text-violet-200"
              title="Open/Advancing plot threads in the project. [linked] in the prompt = tagged on this scene."
            >
              Threads: {preview.thread_count}
              {preview.linked_thread_count > 0 &&
                ` (${preview.linked_thread_count} linked)`}
            </span>
          )}
        </div>
      )}

      {preview.drift_blocks_send && (
        <div className="mb-3 rounded-md border border-red-300 bg-red-50 px-3 py-2 text-xs text-red-900 dark:border-red-900/40 dark:bg-red-950/30 dark:text-red-200">
          <div className="flex items-start gap-2">
            <Shield className="mt-0.5 h-4 w-4 shrink-0" />
            <div>
              <div className="font-semibold">
                Drift gate: voice score{" "}
                {((preview.current_drift ?? 0) * 100).toFixed(0)} ≥ 70
              </div>
              <p className="mt-1 leading-relaxed">
                Your scene's voice has drifted from your reference passages. Asking the
                model to extend it now usually compounds the drift. Pause, re-read the
                pinned references in Research, fix a few lines by hand, and try again.
                If you really want to push through, check the override below.
              </p>
              <label className="mt-2 inline-flex items-center gap-1.5 text-xs">
                <input
                  type="checkbox"
                  checked={overrideDrift}
                  onChange={(e) => onToggleOverride(e.target.checked)}
                />
                Override the drift gate this once
              </label>
            </div>
          </div>
        </div>
      )}

      <details className="rounded-md border border-line-subtle bg-surface-subtle text-xs">
        <summary className="cursor-pointer select-none px-2.5 py-1.5 text-ink-muted">
          {preview.messages.length} message{preview.messages.length === 1 ? "" : "s"} ·
          expand to read
        </summary>
        <div className="max-h-72 overflow-y-auto border-t border-line-subtle">
          {preview.messages.map((m, i) => (
            <MessageRow key={i} message={m} />
          ))}
        </div>
      </details>

      <div className="mt-3 flex gap-2">
        <button
          type="button"
          onClick={onSend}
          disabled={preview.drift_blocks_send && !overrideDrift}
          className="qbtn-primary flex-1 disabled:cursor-not-allowed disabled:opacity-50"
        >
          <Send className="mr-2 h-4 w-4" />
          {preview.drift_blocks_send && !overrideDrift ? "Drift-gated" : "Send"}
        </button>
        <button type="button" onClick={onCancel} className="qbtn-secondary">
          Cancel
        </button>
      </div>
    </section>
  );
}

function MessageRow({ message }: { message: ChatMessage }): JSX.Element {
  return (
    <div className="border-b border-line-subtle px-2.5 py-1.5 last:border-b-0">
      <div className="text-[10px] font-semibold uppercase tracking-wider text-ink-faint">
        {message.role}
      </div>
      <pre className="mt-1 whitespace-pre-wrap break-words font-mono text-[11px] leading-relaxed text-ink-muted">
        {message.content}
      </pre>
    </div>
  );
}

function ResultBlock({
  suggestion,
  operation,
  onAccept,
  onDiscard,
  onRetry,
}: {
  suggestion: DraftSuggestion;
  operation: DraftOperation;
  onAccept: () => void;
  onDiscard: () => void;
  onRetry: () => void;
}): JSX.Element {
  const acceptLabel =
    operation === "continue" || operation === "rewrite" ? "Review changes" : null;
  return (
    <section className="border-b border-line-subtle bg-surface px-4 py-3">
      <div className="mb-2 flex items-center justify-between">
        <h3 className="text-xs font-semibold uppercase tracking-wide text-ink-faint">
          Suggestion
        </h3>
        <span className="text-xs text-ink-faint">
          {suggestion.tokens_in.toLocaleString()} →{" "}
          {suggestion.tokens_out.toLocaleString()} tok · {suggestion.model}
        </span>
      </div>
      <div className="max-h-72 overflow-y-auto rounded-md border border-line-subtle bg-surface-subtle px-3 py-2 font-prose text-sm leading-relaxed text-ink">
        {suggestion.content || <span className="text-ink-faint">(empty response)</span>}
      </div>
      <div className="mt-3 flex gap-2">
        {acceptLabel && (
          <button
            type="button"
            onClick={onAccept}
            className="qbtn-primary flex-1"
            disabled={!suggestion.content.trim()}
          >
            {acceptLabel}
          </button>
        )}
        <button type="button" onClick={onRetry} className="qbtn-secondary">
          <RefreshCw className="mr-1 h-3.5 w-3.5" />
          Retry
        </button>
        <button type="button" onClick={onDiscard} className="qbtn-ghost">
          Discard
        </button>
      </div>
      {suggestion.override_drift_gate && (
        <p className="mt-2 text-xs text-amber-700 dark:text-amber-300">
          Drift gate was overridden for this draft. Audit log records the override.
        </p>
      )}
    </section>
  );
}

// ---------- Helpers ----------

function instructionPlaceholder(op: DraftOperation): string {
  switch (op) {
    case "continue":
      return "e.g. Open with the dragon falling — Kaelan sees the wing first.";
    case "rewrite":
      return "e.g. Tighten this. Same scope, more image, fewer adverbs.";
    case "critique":
      return "e.g. What's flat? Where does the cadence stall?";
  }
}

function truncate(s: string, n: number): string {
  if (s.length <= n) return s;
  return `${s.slice(0, n - 1).trimEnd()}…`;
}

function messageOf(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return JSON.stringify(e);
}
