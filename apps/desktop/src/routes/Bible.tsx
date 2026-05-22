import { ViewHeader } from "@/routes/Manuscript";

export function BibleView(): JSX.Element {
  return (
    <div className="flex h-full flex-col">
      <ViewHeader
        title="Character Bible"
        subtitle="Auto-extracted from your manuscript"
      />
      <PhasePlaceholder phase={7} description="Characters, locations, and lore — derived from the manuscript and editable in place. Continuity flags surface contradictions across chapters." />
    </div>
  );
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
