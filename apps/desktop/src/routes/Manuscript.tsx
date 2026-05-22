import { useApp } from "@/stores/app";

export function ManuscriptView(): JSX.Element {
  const project = useApp((s) => s.currentProject);

  return (
    <div className="flex h-full flex-col">
      <ViewHeader
        title="Manuscript"
        subtitle={project?.name ?? "No project open"}
      />
      <div className="flex flex-1 items-center justify-center p-8">
        <div className="prose-pane max-w-prose text-center text-ink-muted">
          <p className="text-base">
            The writing pane will live here.
          </p>
          <p className="mt-2 text-sm text-ink-subtle">
            Phase 5 brings the Lexical editor, three generation modes, and
            inline track-changes. For now, Phase 0 confirms the foundation:
            theming, project storage, encrypted secrets, and Git auto-commit.
          </p>
        </div>
      </div>
    </div>
  );
}

export function ViewHeader({
  title,
  subtitle,
  right,
}: {
  title: string;
  subtitle?: string;
  right?: React.ReactNode;
}): JSX.Element {
  return (
    <header className="app-chrome flex shrink-0 items-center justify-between border-b border-line-subtle bg-surface-subtle px-5 py-3">
      <div>
        <h1 className="text-sm font-semibold text-ink">{title}</h1>
        {subtitle && (
          <p className="mt-0.5 text-xs text-ink-faint">{subtitle}</p>
        )}
      </div>
      {right}
    </header>
  );
}
