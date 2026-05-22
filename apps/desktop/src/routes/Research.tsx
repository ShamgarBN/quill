import { ViewHeader } from "@/routes/Manuscript";
import { PhasePlaceholder } from "@/routes/Bible";

export function ResearchView(): JSX.Element {
  return (
    <div className="flex h-full flex-col">
      <ViewHeader
        title="Research"
        subtitle="Reference passages and craft notes"
      />
      <PhasePlaceholder
        phase={4}
        description="Pin 3–5 paragraphs from your reference shelf — Eragon, Percy Jackson, Harry Potter, Wingfeather — to condition voice generation."
      />
    </div>
  );
}
