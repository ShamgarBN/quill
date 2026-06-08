import { useState } from "react";
import { useApp } from "@/stores/app";
import { Feather, Plus } from "lucide-react";
import { WindowDragBar } from "@/components/shell/WindowDragBar";
import { errToString } from "@/lib/err";

export function ProjectPicker(): JSX.Element {
  const projects = useApp((s) => s.projects);
  const createProject = useApp((s) => s.createProject);
  const openProject = useApp((s) => s.openProject);

  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  async function handleCreate(): Promise<void> {
    const trimmed = name.trim();
    if (!trimmed) return;
    setBusy(true);
    setErr(null);
    try {
      await createProject(trimmed);
      setName("");
    } catch (e) {
      setErr(errToString(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex h-full flex-col bg-surface">
      <WindowDragBar />
      <div className="flex flex-1 items-center justify-center px-6 py-12">
        <div className="w-full max-w-md">
          <header className="mb-8 flex flex-col items-center text-center">
            <div className="mb-3 flex h-12 w-12 items-center justify-center rounded-full bg-accent-subtle text-accent">
              <Feather className="h-6 w-6" />
            </div>
            <h1 className="text-xl font-semibold text-ink">Welcome to Quill</h1>
            <p className="mt-1 max-w-sm text-sm text-ink-muted">
              A writing companion that learns your voice. Start by naming your book —
              you can change this later.
            </p>
          </header>

          <div className="qcard p-5">
            <label
              htmlFor="project-name"
              className="mb-1.5 block text-xs font-medium uppercase tracking-wider text-ink-muted"
            >
              Working title
            </label>
            <input
              id="project-name"
              type="text"
              autoFocus
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !busy) void handleCreate();
              }}
              placeholder="The Wingfeather…"
              className="qinput"
              disabled={busy}
            />
            {err && (
              <p className="mt-2 text-xs text-danger" role="alert">
                {err}
              </p>
            )}
            <div className="mt-4 flex justify-end">
              <button
                type="button"
                className="qbtn-primary"
                disabled={busy || !name.trim()}
                onClick={() => void handleCreate()}
              >
                <Plus className="h-4 w-4" />
                {busy ? "Creating…" : "Create project"}
              </button>
            </div>
          </div>

          {projects.length > 0 && (
            <div className="mt-6">
              <div className="mb-2 text-xs font-medium uppercase tracking-wider text-ink-muted">
                Recent
              </div>
              <ul className="qcard divide-y divide-line-subtle">
                {projects.map((p) => (
                  <li key={p.id}>
                    <button
                      type="button"
                      onClick={() => void openProject(p.id)}
                      className="flex w-full items-center justify-between px-4 py-3 text-left text-sm transition-colors hover:bg-surface-muted"
                    >
                      <span className="truncate font-medium text-ink">{p.name}</span>
                      <span className="ml-3 text-xs text-ink-faint">
                        {p.manuscript_word_count.toLocaleString()} words
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
