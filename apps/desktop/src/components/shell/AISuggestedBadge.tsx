/**
 * "AI" pill shown on Character / Idea / Thread entries that were created
 * by the canon entity extraction pass.
 *
 * Hover reveals the source doc the entry was extracted from (if known).
 * Tiny on purpose — the user's own entries should still feel like the
 * primary citizens of the view.
 */
import { Sparkles } from "lucide-react";
import { cn } from "@/lib/cn";

export function AISuggestedBadge({
  sourceDocId,
  className,
}: {
  sourceDocId?: string | null;
  className?: string;
}): JSX.Element {
  const title = sourceDocId
    ? `AI-suggested from canon doc ${sourceDocId}`
    : "AI-suggested from canon";
  return (
    <span
      title={title}
      className={cn(
        "inline-flex shrink-0 items-center gap-0.5 rounded-full",
        "border border-accent/30 bg-accent-subtle px-1.5 py-0.5",
        "text-[9px] font-semibold uppercase tracking-wide text-accent",
        className,
      )}
    >
      <Sparkles className="h-2.5 w-2.5" />
      AI
    </span>
  );
}
