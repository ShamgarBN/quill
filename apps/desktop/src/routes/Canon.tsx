/**
 * Canon view — Phase 1.
 *
 * Lets the user pick a file from disk, ingest it (PDF / Markdown / .txt),
 * and run semantic search against the worldbuilding corpus. Designed for
 * lightweight use today and incremental growth as Phases 2+ land.
 *
 * IMPORTANT: this view assumes a project is open. Sidebar guards that.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  AlertCircle,
  CheckCircle2,
  FileText,
  Loader2,
  Search,
  ShieldOff,
  Upload,
} from "lucide-react";
import { useApp } from "@/stores/app";
import * as ipc from "@/lib/ipc";
import type { CanonKind, ChunkRef, ChunkSensitivity, IngestReport } from "@/types";
import { ViewHeader } from "@/routes/Manuscript";
import { cn } from "@/lib/cn";

const KIND_OPTIONS: { value: CanonKind; label: string }[] = [
  { value: "lore", label: "Lore (default)" },
  { value: "character", label: "Character" },
  { value: "location", label: "Location" },
  { value: "faction", label: "Faction" },
  { value: "magic", label: "Magic system" },
  { value: "history", label: "History" },
  { value: "cosmology", label: "Cosmology" },
  { value: "timeline", label: "Timeline" },
  { value: "plot_notes", label: "Plot notes" },
  { value: "dm_notes", label: "DM session notes" },
  { value: "other", label: "Other" },
];

const SENSITIVITY_OPTIONS: {
  value: ChunkSensitivity;
  label: string;
  hint: string;
}[] = [
  {
    value: "public",
    label: "Public",
    hint: "Sent to cloud LLMs as needed for retrieval & generation.",
  },
  {
    value: "spoiler",
    label: "Spoiler",
    hint: "Excluded from early-chapter context unless you opt in per scene.",
  },
  {
    value: "do_not_send",
    label: "Do-not-send",
    hint: "Never transmitted to a cloud provider. Local search only.",
  },
];

export function CanonView(): JSX.Element {
  const project = useApp((s) => s.currentProject);
  const settings = useApp((s) => s.settings);
  const [count, setCount] = useState<number | null>(null);
  const [kind, setKind] = useState<CanonKind>("lore");
  const [sensitivity, setSensitivity] = useState<ChunkSensitivity>("public");
  const [reports, setReports] = useState<IngestReport[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refreshCount = useCallback(async () => {
    if (!project) return;
    try {
      const c = await ipc.canonCount(project.id);
      setCount(c);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(msg);
    }
  }, [project]);

  useEffect(() => {
    void refreshCount();
  }, [refreshCount]);

  const usingMock = settings?.embedding_provider === "mock";

  const handleIngest = useCallback(async () => {
    if (!project) return;
    setError(null);
    try {
      const path = await openDialog({
        multiple: false,
        filters: [
          {
            name: "Canon source",
            extensions: ["md", "markdown", "txt", "pdf"],
          },
        ],
      });
      if (typeof path !== "string") return;
      setBusy(true);
      const report = await ipc.canonIngestFile({
        projectId: project.id,
        path,
        kind,
        sensitivity,
      });
      setReports((r) => [report, ...r].slice(0, 20));
      await refreshCount();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(msg);
    } finally {
      setBusy(false);
    }
  }, [project, kind, sensitivity, refreshCount]);

  if (!project) {
    return (
      <div className="flex h-full flex-col">
        <ViewHeader title="Canon" subtitle="Open a project first" />
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <ViewHeader
        title="Canon"
        subtitle={`${count ?? "—"} chunks indexed`}
        right={
          <button
            type="button"
            className="qbtn-primary inline-flex items-center gap-2"
            disabled={busy}
            onClick={() => void handleIngest()}
          >
            {busy ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <Upload className="h-4 w-4" />
            )}
            Ingest file
          </button>
        }
      />

      <div className="flex min-h-0 flex-1 flex-col gap-6 overflow-y-auto p-6">
        {usingMock && <MockEmbeddingsBanner />}
        {error && <ErrorBanner message={error} onDismiss={() => setError(null)} />}

        <section>
          <SectionTitle>New ingest defaults</SectionTitle>
          <p className="mt-1 max-w-prose text-xs text-ink-faint">
            These apply to the next file you ingest. You can still override per chunk
            later, but tagging at ingest time saves a lot of tedium.
          </p>
          <div className="mt-3 grid grid-cols-1 gap-4 md:grid-cols-2">
            <Field label="Kind">
              <select
                className="qinput"
                value={kind}
                onChange={(e) => setKind(e.target.value as CanonKind)}
              >
                {KIND_OPTIONS.map((o) => (
                  <option key={o.value} value={o.value}>
                    {o.label}
                  </option>
                ))}
              </select>
            </Field>
            <Field label="Sensitivity">
              <select
                className="qinput"
                value={sensitivity}
                onChange={(e) => setSensitivity(e.target.value as ChunkSensitivity)}
              >
                {SENSITIVITY_OPTIONS.map((o) => (
                  <option key={o.value} value={o.value}>
                    {o.label}
                  </option>
                ))}
              </select>
              <p className="mt-1 text-xs text-ink-faint">
                {SENSITIVITY_OPTIONS.find((o) => o.value === sensitivity)?.hint}
              </p>
            </Field>
          </div>
        </section>

        <section>
          <SectionTitle>Search canon</SectionTitle>
          <p className="mt-1 max-w-prose text-xs text-ink-faint">
            Semantic retrieval over everything you've ingested. Uses the{" "}
            {settings?.embedding_provider} embeddings provider.
          </p>
          <CanonSearch projectId={project.id} />
        </section>

        {reports.length > 0 && (
          <section>
            <SectionTitle>Recent ingests</SectionTitle>
            <div className="mt-3 flex flex-col gap-2">
              {reports.map((r) => (
                <ReportRow key={r.document.id} report={r} />
              ))}
            </div>
          </section>
        )}
      </div>
    </div>
  );
}

// ---------- Subcomponents ----------

function CanonSearch({ projectId }: { projectId: string }): JSX.Element {
  const [q, setQ] = useState("");
  const [k, setK] = useState(5);
  const [respectDoNotSend, setRespectDoNotSend] = useState(true);
  const [hits, setHits] = useState<ChunkRef[]>([]);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const submit = useCallback(async () => {
    if (!q.trim()) return;
    setBusy(true);
    setErr(null);
    try {
      const out = await ipc.canonSearch({
        projectId,
        query: q,
        k,
        respectDoNotSend,
      });
      setHits(out);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setErr(msg);
    } finally {
      setBusy(false);
    }
  }, [projectId, q, k, respectDoNotSend]);

  return (
    <div className="mt-3 flex flex-col gap-3">
      <div className="flex gap-2">
        <div className="relative flex-1">
          <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-ink-faint" />
          <input
            type="text"
            className="qinput pl-9"
            placeholder="e.g. the dragon's lair, or House Vell rivalries"
            value={q}
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void submit();
            }}
          />
        </div>
        <select
          className="qinput w-20"
          value={k}
          onChange={(e) => setK(Number(e.target.value))}
        >
          {[3, 5, 8, 12].map((n) => (
            <option key={n} value={n}>
              k={n}
            </option>
          ))}
        </select>
        <button
          type="button"
          className="qbtn-primary"
          disabled={busy || !q.trim()}
          onClick={() => void submit()}
        >
          {busy ? "Searching…" : "Search"}
        </button>
      </div>
      <label className="inline-flex select-none items-center gap-2 text-xs text-ink-muted">
        <input
          type="checkbox"
          checked={respectDoNotSend}
          onChange={(e) => setRespectDoNotSend(e.target.checked)}
        />
        Respect do-not-send sensitivity
      </label>

      {err && <ErrorBanner message={err} onDismiss={() => setErr(null)} />}
      {hits.length === 0 && !busy && q && !err && (
        <div className="rounded-md border border-line-subtle bg-surface-subtle px-4 py-6 text-center text-sm text-ink-faint">
          No matches yet. Try a different query, or ingest more material.
        </div>
      )}
      <div className="flex flex-col gap-2">
        {hits.map((h) => (
          <ChunkCard key={h.id} hit={h} />
        ))}
      </div>
    </div>
  );
}

function ChunkCard({ hit }: { hit: ChunkRef }): JSX.Element {
  return (
    <div className="rounded-md border border-line-subtle bg-surface-subtle p-3">
      <div className="flex items-baseline justify-between gap-3">
        <div className="min-w-0 truncate text-xs font-medium text-ink-muted">
          {hit.headings.length > 0 ? hit.headings.join(" ▸ ") : "(no heading)"}
        </div>
        <div className="shrink-0 text-xs tabular-nums text-ink-faint">
          score {hit.score.toFixed(3)}
        </div>
      </div>
      <p className="mt-2 max-h-48 overflow-y-auto whitespace-pre-wrap text-sm leading-relaxed text-ink">
        {hit.text}
      </p>
      <div className="mt-2 flex items-center gap-2 text-xs text-ink-faint">
        <span>
          chunk #{hit.index} · {hit.word_count} words
        </span>
        {hit.sensitivity !== "public" && (
          <span
            className={cn(
              "inline-flex items-center gap-1 rounded-full px-2 py-0.5",
              hit.sensitivity === "do_not_send"
                ? "bg-rose-100 text-rose-800 dark:bg-rose-900/30 dark:text-rose-300"
                : "bg-amber-100 text-amber-800 dark:bg-amber-900/30 dark:text-amber-300",
            )}
          >
            {hit.sensitivity === "do_not_send" ? (
              <ShieldOff className="h-3 w-3" />
            ) : null}
            {hit.sensitivity === "do_not_send" ? "do not send" : "spoiler"}
          </span>
        )}
      </div>
    </div>
  );
}

function ReportRow({ report }: { report: IngestReport }): JSX.Element {
  const filename = useMemo(() => {
    const path = report.document.source_path;
    return path.split("/").pop() ?? path;
  }, [report]);

  return (
    <div className="flex items-center gap-3 rounded-md border border-line-subtle bg-surface-subtle px-3 py-2 text-sm">
      <CheckCircle2 className="h-4 w-4 shrink-0 text-emerald-600 dark:text-emerald-400" />
      <FileText className="h-4 w-4 shrink-0 text-ink-faint" />
      <div className="min-w-0 flex-1 truncate">
        <div className="truncate font-medium text-ink">{filename}</div>
        <div className="truncate text-xs text-ink-faint">
          {report.document.source_path}
        </div>
      </div>
      <div className="shrink-0 text-right text-xs text-ink-faint">
        <div>{report.chunks_emitted} chunks</div>
        <div>{(report.bytes_read / 1024).toFixed(1)} KB</div>
      </div>
    </div>
  );
}

function MockEmbeddingsBanner(): JSX.Element {
  return (
    <div className="flex items-start gap-3 rounded-md border border-amber-300 bg-amber-50 px-4 py-3 text-sm text-amber-900 dark:border-amber-700/40 dark:bg-amber-900/20 dark:text-amber-200">
      <AlertCircle className="mt-0.5 h-4 w-4 shrink-0" />
      <div>
        <div className="font-medium">Mock embeddings active</div>
        <p className="mt-1 text-xs">
          You're using deterministic placeholder embeddings — useful for development,
          but recall is term-bag-shaped, not semantic. Switch to Gemini embeddings in
          Settings → Privacy when you're ready to use a cloud provider.
        </p>
      </div>
    </div>
  );
}

function ErrorBanner({
  message,
  onDismiss,
}: {
  message: string;
  onDismiss: () => void;
}): JSX.Element {
  return (
    <div className="flex items-start gap-3 rounded-md border border-rose-300 bg-rose-50 px-4 py-3 text-sm text-rose-900 dark:border-rose-700/40 dark:bg-rose-900/20 dark:text-rose-200">
      <AlertCircle className="mt-0.5 h-4 w-4 shrink-0" />
      <div className="flex-1">
        <div className="font-medium">Something went wrong</div>
        <p className="mt-1 break-words text-xs">{message}</p>
      </div>
      <button type="button" className="qbtn-ghost text-xs" onClick={onDismiss}>
        dismiss
      </button>
    </div>
  );
}

function SectionTitle({ children }: { children: React.ReactNode }): JSX.Element {
  return (
    <h2 className="text-xs font-semibold uppercase tracking-wider text-ink-faint">
      {children}
    </h2>
  );
}

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}): JSX.Element {
  return (
    <label className="flex flex-col gap-1.5 text-xs">
      <span className="font-medium text-ink-muted">{label}</span>
      {children}
    </label>
  );
}
