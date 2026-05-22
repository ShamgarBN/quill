import { ViewHeader } from "@/routes/Manuscript";

const SAVE_THE_CAT_BEATS = [
  "Opening Image",
  "Theme Stated",
  "Setup",
  "Catalyst",
  "Debate",
  "Break Into Two",
  "B Story",
  "Fun and Games",
  "Midpoint",
  "Bad Guys Close In",
  "All Is Lost",
  "Dark Night of the Soul",
  "Break Into Three",
  "Finale",
  "Final Image",
] as const;

export function BeatsView(): JSX.Element {
  return (
    <div className="flex h-full flex-col">
      <ViewHeader
        title="Beat Sheet"
        subtitle="Save the Cat Writes a Novel — 15 beats"
      />
      <div className="flex-1 overflow-auto p-6">
        <div className="mx-auto max-w-3xl">
          <p className="mb-6 text-sm text-ink-muted">
            Phase 3 will make these interactive — assign target word counts,
            link chapters, lock individual beats as they crystallize, and see a
            structural-health score for your manuscript.
          </p>
          <ol className="qcard divide-y divide-line-subtle">
            {SAVE_THE_CAT_BEATS.map((beat, i) => (
              <li
                key={beat}
                className="flex items-center justify-between px-4 py-3"
              >
                <span className="flex items-center gap-3">
                  <span className="flex h-6 w-6 items-center justify-center rounded-full bg-surface-muted text-xs font-medium text-ink-muted">
                    {i + 1}
                  </span>
                  <span className="text-sm text-ink">{beat}</span>
                </span>
                <span className="qbadge">empty</span>
              </li>
            ))}
          </ol>
        </div>
      </div>
    </div>
  );
}
