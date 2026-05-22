import { ViewHeader } from "@/routes/Manuscript";
import { PhasePlaceholder } from "@/routes/Bible";

export function IdeasView(): JSX.Element {
  return (
    <div className="flex h-full flex-col">
      <ViewHeader title="Idea Park" subtitle="Stray ideas, captured fast" />
      <PhasePlaceholder
        phase={7}
        description="A scratchpad for ideas not yet placed in the manuscript. One-key send-to-scene from any entry."
      />
    </div>
  );
}
