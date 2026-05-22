import { AlertTriangle } from "lucide-react";

export function BootError({ message }: { message: string }): JSX.Element {
  return (
    <div className="flex h-full items-center justify-center bg-surface px-6">
      <div className="qcard max-w-md p-6">
        <div className="mb-3 flex items-center gap-2 text-danger">
          <AlertTriangle className="h-5 w-5" />
          <span className="text-sm font-semibold">Startup error</span>
        </div>
        <p className="text-sm text-ink-muted">{message}</p>
        <p className="mt-3 text-xs text-ink-faint">
          Quitting and reopening Quill usually resolves this. If it persists,
          open <code>~/Library/Application Support/Quill/audit.log</code> for
          details.
        </p>
      </div>
    </div>
  );
}
