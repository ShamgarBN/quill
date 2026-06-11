/**
 * Canon view — Phase 1.
 *
 * Lets the user pick a file from disk, ingest it (PDF / Markdown / .txt),
 * and run semantic search against the worldbuilding corpus. Designed for
 * lightweight use today and incremental growth as Phases 2+ land.
 *
 * IMPORTANT: this view assumes a project is open. Sidebar guards that.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import {
  AlertCircle,
  CheckCircle2,
  Eye,
  EyeOff,
  FileText,
  FileWarning,
  FolderOpen,
  FolderSearch,
  Loader2,
  Plus,
  RefreshCw,
  Search,
  Shield,
  ShieldOff,
  Sparkles,
  Trash2,
  Upload,
} from "lucide-react";
import { useApp } from "@/stores/app";
import * as ipc from "@/lib/ipc";
import type {
  CanonKind,
  ChunkRef,
  ChunkSensitivity,
  DocSummary,
  ExtractionCompleteEvent,
  IngestReport,
  VaultRule,
  WatchStatus,
} from "@/types";
import { ViewHeader } from "@/routes/Manuscript";
import { cn } from "@/lib/cn";
import { errToString } from "@/lib/err";

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
      const msg = errToString(e);
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
      const msg = errToString(e);
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
          <SectionTitle>Obsidian vault</SectionTitle>
          <p className="mt-1 max-w-prose text-xs text-ink-faint">
            Point Quill at an Obsidian (or any) directory. When watching is on, files
            you save in that directory are re-ingested automatically. New files are
            picked up too; deletions are ignored (Obsidian saves atomically by writing a
            temp file, so you'd lose data if we acted on Removed events).
          </p>
          <VaultWatcherCard projectId={project.id} />
        </section>

        <section>
          <SectionTitle>Privacy rules</SectionTitle>
          <p className="mt-1 max-w-prose text-xs text-ink-faint">
            Map folders (or path prefixes) to sensitivity tiers. Files in a matching
            folder get that tier on every re-ingest. A note's own YAML frontmatter (
            <code className="text-ink-muted">quill-sensitivity: do_not_send</code>)
            overrides folder rules. Anything unmatched falls back to the default below.
          </p>
          <VaultPrivacyCard />
        </section>

        <section>
          <SectionTitle>Indexed documents</SectionTitle>
          <p className="mt-1 max-w-prose text-xs text-ink-faint">
            Everything currently in your project's canon index — what the AI can pull
            from when drafting. Use this to audit sensitivity before turning on a cloud
            provider, bulk-retag a batch of files, or prune chunks for notes you've
            deleted from your vault.
          </p>
          <CorpusInspector projectId={project.id} />
        </section>

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
      const msg = errToString(e);
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

type SensFilter = "all" | ChunkSensitivity;

const SENSITIVITY_TONE: Record<ChunkSensitivity, string> = {
  public: "text-sky-700 dark:text-sky-300",
  spoiler: "text-amber-700 dark:text-amber-300",
  do_not_send: "text-rose-700 dark:text-rose-300",
};

const SENSITIVITY_LABEL: Record<ChunkSensitivity, string> = {
  public: "Public",
  spoiler: "Spoiler",
  do_not_send: "Do not send",
};

function CorpusInspector({ projectId }: { projectId: string }): JSX.Element {
  const [docs, setDocs] = useState<DocSummary[]>([]);
  const [loading, setLoading] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [filter, setFilter] = useState<SensFilter>("all");
  const [showMissingOnly, setShowMissingOnly] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [bulkBusy, setBulkBusy] = useState(false);
  const [bulkTarget, setBulkTarget] = useState<ChunkSensitivity>("do_not_send");
  const [status, setStatus] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setErr(null);
    try {
      const list = await ipc.canonListDocuments(projectId);
      setDocs(list);
      // Drop stale selections.
      setSelected((curr) => {
        const valid = new Set(list.map((d) => d.doc_id));
        const next = new Set<string>();
        curr.forEach((id) => valid.has(id) && next.add(id));
        return next;
      });
    } catch (e) {
      setErr(errToString(e));
    } finally {
      setLoading(false);
    }
  }, [projectId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // Track which docs are currently being extracted so each row can
  // show a spinner. The `extracting` Set is fed by two Tauri events:
  // `canon-extraction-started` adds, `canon-extraction-complete` removes.
  // This covers both manual (Re-extract button) and auto (post-ingest)
  // triggers — the user always gets a visible "in progress" signal.
  const [extracting, setExtracting] = useState<Set<string>>(new Set());
  useEffect(() => {
    let unlistenStart: (() => void) | undefined;
    let unlistenDone: (() => void) | undefined;
    void listen<{ doc_id: string }>("canon-extraction-started", (e) => {
      setExtracting((curr) => new Set(curr).add(e.payload.doc_id));
      setErr(null);
    }).then((fn) => {
      unlistenStart = fn;
    });
    void listen<ExtractionCompleteEvent>("canon-extraction-complete", (e) => {
      const { doc_id, report, error } = e.payload;
      setExtracting((curr) => {
        const next = new Set(curr);
        next.delete(doc_id);
        return next;
      });
      if (error) {
        setErr(error);
      } else if (report.skipped_do_not_send) {
        setStatus("Extraction skipped — all chunks marked do-not-send.");
      } else {
        const added =
          report.characters_added + report.world_added + report.threads_added;
        const enriched = report.characters_enriched + report.world_enriched;
        const returned =
          report.characters_returned + report.world_returned + report.threads_returned;
        const chunksNote = `Read ${report.chunks_sent}/${report.chunks_total} chunks${report.truncated ? " (truncated)" : ""}.`;
        if (added > 0 || enriched > 0) {
          const enrichedNote =
            enriched > 0
              ? ` Updated ${enriched} existing entr${enriched === 1 ? "y" : "ies"} with new facts.`
              : "";
          setStatus(
            `Added ${report.characters_added} character${report.characters_added === 1 ? "" : "s"}, ${report.world_added} world entr${report.world_added === 1 ? "y" : "ies"}, ${report.threads_added} thread${report.threads_added === 1 ? "" : "s"}.${enrichedNote} ${chunksNote}`,
          );
        } else if (returned > 0) {
          setStatus(
            `Model returned ${returned} candidate${returned === 1 ? "" : "s"} but all matched existing entries (no new facts). ${chunksNote}`,
          );
        } else {
          setStatus(
            `Model returned no entities. ${chunksNote} Check Settings → Audit log for details.`,
          );
        }
      }
      void refresh();
    }).then((fn) => {
      unlistenDone = fn;
    });
    return () => {
      if (unlistenStart) unlistenStart();
      if (unlistenDone) unlistenDone();
    };
  }, [refresh]);

  const onToggleExtraction = async (docId: string, enabled: boolean): Promise<void> => {
    try {
      await ipc.canonSetDocExtraction(projectId, docId, enabled);
      await refresh();
    } catch (e) {
      setErr(errToString(e));
    }
  };

  const onReExtract = async (docId: string): Promise<void> => {
    try {
      setExtracting((curr) => new Set(curr).add(docId));
      await ipc.canonExtractDoc(projectId, docId);
      // Status will land via the event listener.
    } catch (e) {
      setExtracting((curr) => {
        const next = new Set(curr);
        next.delete(docId);
        return next;
      });
      setErr(errToString(e));
    }
  };

  const filtered = useMemo(
    () =>
      docs.filter((d) => {
        if (filter !== "all" && d.sensitivity !== filter) return false;
        if (showMissingOnly && d.exists_on_disk) return false;
        return true;
      }),
    [docs, filter, showMissingOnly],
  );

  const totalChunks = docs.reduce((acc, d) => acc + d.chunk_count, 0);
  const totalWords = docs.reduce((acc, d) => acc + d.word_count, 0);
  const missingCount = docs.filter((d) => !d.exists_on_disk).length;

  const toggleOne = (docId: string): void => {
    setSelected((curr) => {
      const next = new Set(curr);
      if (next.has(docId)) next.delete(docId);
      else next.add(docId);
      return next;
    });
  };
  const toggleAll = (): void => {
    setSelected((curr) => {
      const allShown = new Set(filtered.map((d) => d.doc_id));
      const allAlreadySelected =
        filtered.length > 0 && filtered.every((d) => curr.has(d.doc_id));
      if (allAlreadySelected) {
        const next = new Set(curr);
        allShown.forEach((id) => next.delete(id));
        return next;
      }
      return new Set([...curr, ...allShown]);
    });
  };

  const onDelete = async (docId: string): Promise<void> => {
    if (!window.confirm("Remove this document's chunks from the index?")) return;
    try {
      await ipc.canonDeleteDocument(projectId, docId);
      setStatus("Document removed.");
      await refresh();
    } catch (e) {
      setErr(errToString(e));
    }
  };

  const onPrune = async (): Promise<void> => {
    if (
      !window.confirm(
        `Prune all docs whose source file no longer exists on disk? (${missingCount} doc${missingCount === 1 ? "" : "s"} affected.)`,
      )
    )
      return;
    try {
      const pruned = await ipc.canonPruneMissing(projectId);
      setStatus(`${pruned} doc${pruned === 1 ? "" : "s"} pruned.`);
      await refresh();
    } catch (e) {
      setErr(errToString(e));
    }
  };

  const onBulkRetag = async (): Promise<void> => {
    if (selected.size === 0) return;
    setBulkBusy(true);
    setStatus(null);
    try {
      const changed = await ipc.canonRetagDocuments(
        projectId,
        Array.from(selected),
        bulkTarget,
      );
      setStatus(
        changed > 0
          ? `Re-tagged ${changed} chunk${changed === 1 ? "" : "s"} → ${SENSITIVITY_LABEL[bulkTarget]}.`
          : "No chunks needed re-tagging.",
      );
      await refresh();
    } catch (e) {
      setErr(errToString(e));
    } finally {
      setBulkBusy(false);
    }
  };

  return (
    <div className="mt-3 flex flex-col gap-3 rounded-md border border-line-subtle bg-surface-subtle">
      {/* Toolbar */}
      <div className="flex flex-wrap items-center gap-2 border-b border-line-subtle px-3 py-2 text-xs">
        <span className="text-ink-muted">
          {docs.length} doc{docs.length === 1 ? "" : "s"} · {totalChunks} chunks ·{" "}
          {totalWords.toLocaleString()} words
        </span>
        <div className="ml-auto flex items-center gap-1.5">
          <select
            value={filter}
            onChange={(e) => setFilter(e.target.value as SensFilter)}
            className="qinput h-7 px-1.5 text-xs"
            title="Filter by sensitivity"
          >
            <option value="all">All sensitivities</option>
            <option value="public">Public only</option>
            <option value="spoiler">Spoiler only</option>
            <option value="do_not_send">Do not send only</option>
          </select>
          <label
            className={cn(
              "inline-flex items-center gap-1 rounded-full border px-2 py-0.5 transition-colors",
              showMissingOnly
                ? "border-rose-300 bg-rose-50 text-rose-800 dark:border-rose-900/40 dark:bg-rose-900/20 dark:text-rose-200"
                : "border-line-subtle text-ink-faint hover:text-ink",
            )}
            title="Show only documents whose source file is missing on disk"
          >
            <input
              type="checkbox"
              className="hidden"
              checked={showMissingOnly}
              onChange={(e) => setShowMissingOnly(e.target.checked)}
            />
            <FileWarning className="h-3 w-3" />
            Missing only ({missingCount})
          </label>
          <button
            type="button"
            onClick={() => void refresh()}
            disabled={loading}
            className="qbtn-ghost h-7 w-7 p-0"
            title="Refresh"
            aria-label="Refresh document list"
          >
            {loading ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <RefreshCw className="h-3.5 w-3.5" />
            )}
          </button>
          {missingCount > 0 && (
            <button
              type="button"
              onClick={() => void onPrune()}
              className="qbtn-ghost h-7 px-2 text-xs"
              title="Delete chunks for files that no longer exist on disk"
            >
              <Trash2 className="mr-1 h-3 w-3" /> Prune missing
            </button>
          )}
        </div>
      </div>

      {/* Extraction-in-progress banner */}
      {extracting.size > 0 && (
        <div className="flex items-center gap-2 border-b border-accent/30 bg-accent-subtle px-3 py-1.5 text-xs text-accent">
          <Loader2 className="h-3 w-3 animate-spin" />
          <span className="font-medium">
            AI extraction running for {extracting.size} doc
            {extracting.size === 1 ? "" : "s"}…
          </span>
          <span className="text-ink-muted">
            Reading chunks, identifying characters / locations / lore / threads.
          </span>
        </div>
      )}

      {/* Status / error banner */}
      {(status || err) && (
        <div
          className={cn(
            "flex items-center gap-2 border-b px-3 py-1.5 text-xs",
            err
              ? "border-rose-200 bg-rose-50 text-rose-900 dark:border-rose-900/40 dark:bg-rose-950/40 dark:text-rose-200"
              : "border-emerald-200 bg-emerald-50 text-emerald-900 dark:border-emerald-900/40 dark:bg-emerald-950/40 dark:text-emerald-200",
          )}
        >
          {err ? (
            <AlertCircle className="h-3 w-3 shrink-0" />
          ) : (
            <CheckCircle2 className="h-3 w-3 shrink-0" />
          )}
          <span className="flex-1">{err ?? status}</span>
          <button
            type="button"
            onClick={() => {
              setErr(null);
              setStatus(null);
            }}
            className="text-current opacity-60 hover:opacity-100"
            aria-label="Dismiss"
          >
            ×
          </button>
        </div>
      )}

      {/* Bulk action bar */}
      {selected.size > 0 && (
        <div className="flex items-center gap-2 border-b border-line-subtle px-3 py-2 text-xs">
          <span className="font-medium text-ink">{selected.size} selected</span>
          <span className="text-ink-faint">retag to</span>
          <select
            value={bulkTarget}
            onChange={(e) => setBulkTarget(e.target.value as ChunkSensitivity)}
            className="qinput h-7 px-1.5 text-xs"
          >
            <option value="public">Public</option>
            <option value="spoiler">Spoiler</option>
            <option value="do_not_send">Do not send</option>
          </select>
          <button
            type="button"
            onClick={() => void onBulkRetag()}
            disabled={bulkBusy}
            className="qbtn-primary h-7 px-3 text-xs"
          >
            {bulkBusy ? <Loader2 className="mr-1 h-3 w-3 animate-spin" /> : null}
            Apply
          </button>
          <button
            type="button"
            onClick={() => setSelected(new Set())}
            className="qbtn-ghost h-7 px-2 text-xs"
          >
            Clear
          </button>
        </div>
      )}

      {/* List */}
      {filtered.length === 0 ? (
        <div className="px-3 py-6 text-center text-xs text-ink-faint">
          {loading
            ? "Loading…"
            : docs.length === 0
              ? "No documents indexed yet. Ingest a file or connect a vault to begin."
              : "No documents match this filter."}
        </div>
      ) : (
        <>
          <div className="flex items-center gap-2 border-b border-line-subtle px-3 py-1.5 text-[10px] uppercase tracking-wide text-ink-faint">
            <input
              type="checkbox"
              checked={
                filtered.length > 0 && filtered.every((d) => selected.has(d.doc_id))
              }
              onChange={toggleAll}
            />
            <span className="flex-1">Source</span>
            <span className="w-20 text-right">Words</span>
            <span className="w-14 text-right">Chunks</span>
            <span className="w-24 text-center">Sensitivity</span>
            <span className="w-32" />
          </div>
          <ul className="max-h-[480px] divide-y divide-line-subtle overflow-y-auto">
            {filtered.map((doc) => (
              <DocRow
                key={doc.doc_id}
                doc={doc}
                selected={selected.has(doc.doc_id)}
                extracting={extracting.has(doc.doc_id)}
                onToggle={() => toggleOne(doc.doc_id)}
                onDelete={() => void onDelete(doc.doc_id)}
                onToggleExtraction={(enabled) =>
                  void onToggleExtraction(doc.doc_id, enabled)
                }
                onReExtract={() => void onReExtract(doc.doc_id)}
              />
            ))}
          </ul>
        </>
      )}
    </div>
  );
}

function DocRow({
  doc,
  selected,
  extracting,
  onToggle,
  onDelete,
  onToggleExtraction,
  onReExtract,
}: {
  doc: DocSummary;
  selected: boolean;
  extracting: boolean;
  onToggle: () => void;
  onDelete: () => void;
  onToggleExtraction: (enabled: boolean) => void;
  onReExtract: () => void;
}): JSX.Element {
  const filename = doc.source_path.split("/").pop() || "(no path)";
  const parent = doc.source_path
    .split("/")
    .slice(0, -1)
    .join("/")
    .replace(/^\/Users\/[^/]+/, "~");
  return (
    <li
      className={cn(
        "group flex items-center gap-2 px-3 py-1.5 text-xs",
        selected && "bg-accent-subtle",
        !doc.exists_on_disk && "bg-rose-50/40 dark:bg-rose-900/10",
      )}
    >
      <input type="checkbox" checked={selected} onChange={onToggle} />
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-1.5">
          <FileText className="h-3 w-3 shrink-0 text-ink-faint" />
          <span className="truncate font-medium text-ink">{filename}</span>
          {!doc.exists_on_disk && (
            <span
              className="inline-flex items-center gap-0.5 rounded-full bg-rose-100 px-1.5 py-0.5 text-[10px] text-rose-800 dark:bg-rose-900/30 dark:text-rose-200"
              title="Source file no longer exists on disk"
            >
              <FileWarning className="h-2.5 w-2.5" /> missing
            </span>
          )}
          {doc.mixed_sensitivity && (
            <span
              className="inline-flex items-center gap-0.5 rounded-full bg-amber-100 px-1.5 py-0.5 text-[10px] text-amber-900 dark:bg-amber-900/30 dark:text-amber-200"
              title="Chunks of this document have different sensitivity tags"
            >
              mixed
            </span>
          )}
        </div>
        <div className="truncate text-ink-faint">{parent || "(root)"}</div>
      </div>
      <span className="w-20 text-right tabular-nums text-ink-muted">
        {doc.word_count.toLocaleString()}
      </span>
      <span className="w-14 text-right tabular-nums text-ink-muted">
        {doc.chunk_count}
      </span>
      <span
        className={cn(
          "w-24 text-center font-medium",
          SENSITIVITY_TONE[doc.sensitivity],
        )}
      >
        {SENSITIVITY_LABEL[doc.sensitivity]}
      </span>
      <div className="flex w-32 items-center justify-end gap-0.5">
        <button
          type="button"
          onClick={() => onToggleExtraction(!doc.extraction_enabled)}
          className={cn(
            "qbtn-ghost h-6 w-6 p-0",
            doc.extraction_enabled
              ? "text-accent"
              : "text-ink-faint opacity-60 hover:opacity-100",
          )}
          title={
            doc.extraction_enabled
              ? `AI extraction ON${doc.last_extracted_at ? ` · last run ${formatRelative(doc.last_extracted_at)}` : " · never run"}`
              : "AI extraction OFF — click to enable"
          }
          aria-label="Toggle AI extraction for this document"
        >
          <Sparkles className="h-3 w-3" />
        </button>
        <button
          type="button"
          onClick={onReExtract}
          disabled={extracting || doc.sensitivity === "do_not_send"}
          className="qbtn-ghost h-6 px-1.5 text-[10px] opacity-0 group-hover:opacity-100 disabled:opacity-30"
          title={
            doc.sensitivity === "do_not_send"
              ? "Cannot extract from do-not-send chunks"
              : "Re-run AI extraction for this document"
          }
          aria-label="Re-run AI extraction"
        >
          {extracting ? <Loader2 className="h-3 w-3 animate-spin" /> : "Extract"}
        </button>
        {doc.source_path && doc.exists_on_disk && (
          <button
            type="button"
            onClick={() =>
              void ipc.systemRevealPath(doc.source_path).catch(() => undefined)
            }
            className="qbtn-ghost h-6 w-6 p-0 opacity-0 group-hover:opacity-100"
            title={`Reveal in Finder · ${doc.source_path}`}
            aria-label="Reveal in Finder"
          >
            <FolderSearch className="h-3 w-3" />
          </button>
        )}
        <button
          type="button"
          onClick={onDelete}
          className="qbtn-ghost h-6 w-6 p-0 text-ink-faint opacity-0 group-hover:opacity-100 hover:text-rose-600"
          title="Remove from index"
          aria-label="Remove document from index"
        >
          <Trash2 className="h-3 w-3" />
        </button>
      </div>
    </li>
  );
}

function VaultPrivacyCard(): JSX.Element {
  const project = useApp((s) => s.currentProject);
  const updateCurrentProject = useApp((s) => s.updateCurrentProject);

  // Local draft state so the user can stage multiple edits before saving.
  const [rules, setRules] = useState<VaultRule[]>(project?.vault_rules ?? []);
  const [defaultSens, setDefaultSens] = useState<ChunkSensitivity>(
    project?.vault_default_sensitivity ?? "public",
  );
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [status, setStatus] = useState<{
    kind: "ok" | "err";
    message: string;
  } | null>(null);

  // Re-sync local state when the project itself changes.
  useEffect(() => {
    setRules(project?.vault_rules ?? []);
    setDefaultSens(project?.vault_default_sensitivity ?? "public");
    setDirty(false);
    setStatus(null);
  }, [project?.id, project?.vault_rules, project?.vault_default_sensitivity]);

  const addRule = (): void => {
    setRules((r) => [...r, { pattern: "", sensitivity: "do_not_send" }]);
    setDirty(true);
  };
  const updateRule = (i: number, patch: Partial<VaultRule>): void => {
    setRules((r) => r.map((rule, idx) => (idx === i ? { ...rule, ...patch } : rule)));
    setDirty(true);
  };
  const removeRule = (i: number): void => {
    setRules((r) => r.filter((_, idx) => idx !== i));
    setDirty(true);
  };

  const save = async (): Promise<void> => {
    if (!project) return;
    setSaving(true);
    setStatus(null);
    try {
      // Validate: strip empty patterns.
      const cleaned = rules.filter((r) => r.pattern.trim() !== "");
      await updateCurrentProject({
        vault_rules: cleaned,
        vault_default_sensitivity: defaultSens,
      });
      const changed = await ipc.canonReapplyRules(project.id);
      setDirty(false);
      setStatus({
        kind: "ok",
        message:
          changed > 0
            ? `Saved. ${changed} existing chunk${changed === 1 ? "" : "s"} re-tagged.`
            : "Saved. No existing chunks needed re-tagging.",
      });
    } catch (e) {
      setStatus({
        kind: "err",
        message: errToString(e),
      });
    } finally {
      setSaving(false);
    }
  };

  if (!project) return <></>;

  return (
    <div className="mt-3 flex flex-col gap-3 rounded-md border border-line-subtle bg-surface-subtle p-4">
      {rules.length === 0 ? (
        <div className="text-xs text-ink-faint">
          No rules yet. Add one below to protect a folder of your vault. For example: a
          rule with pattern <code className="text-ink-muted">DM-Notes</code> and
          sensitivity <code className="text-ink-muted">do_not_send</code> protects any
          file in a folder named <code className="text-ink-muted">DM-Notes</code>
          anywhere in your vault.
        </div>
      ) : (
        <ul className="flex flex-col gap-2">
          {rules.map((rule, i) => (
            <li key={i} className="flex items-center gap-2">
              <Shield className="h-3.5 w-3.5 shrink-0 text-ink-faint" />
              <input
                type="text"
                value={rule.pattern}
                onChange={(e) => updateRule(i, { pattern: e.target.value })}
                placeholder="folder name or path/prefix"
                className="qinput h-7 flex-1 px-2 text-xs"
              />
              <select
                value={rule.sensitivity}
                onChange={(e) =>
                  updateRule(i, { sensitivity: e.target.value as ChunkSensitivity })
                }
                className="qinput h-7 px-1.5 text-xs"
              >
                <option value="public">Public</option>
                <option value="spoiler">Spoiler</option>
                <option value="do_not_send">Do not send</option>
              </select>
              <button
                type="button"
                onClick={() => removeRule(i)}
                className="qbtn-ghost h-7 w-7 p-0 text-ink-faint hover:text-rose-600"
                title="Remove rule"
                aria-label="Remove rule"
              >
                <Trash2 className="h-3.5 w-3.5" />
              </button>
            </li>
          ))}
        </ul>
      )}

      <button
        type="button"
        onClick={addRule}
        className="qbtn-ghost inline-flex h-7 self-start items-center gap-1 px-2 text-xs"
      >
        <Plus className="h-3.5 w-3.5" /> Add rule
      </button>

      <div className="flex items-center justify-between gap-3 border-t border-line-subtle pt-3">
        <label className="inline-flex items-center gap-2 text-xs text-ink-muted">
          <span>Default for unmatched files</span>
          <select
            value={defaultSens}
            onChange={(e) => {
              setDefaultSens(e.target.value as ChunkSensitivity);
              setDirty(true);
            }}
            className="qinput h-7 px-1.5 text-xs"
          >
            <option value="public">Public</option>
            <option value="spoiler">Spoiler</option>
            <option value="do_not_send">Do not send</option>
          </select>
        </label>
        <div className="flex items-center gap-2">
          {status && (
            <span
              className={cn(
                "text-xs",
                status.kind === "ok"
                  ? "text-emerald-700 dark:text-emerald-300"
                  : "text-rose-700 dark:text-rose-300",
              )}
            >
              {status.message}
            </span>
          )}
          <button
            type="button"
            onClick={() => void save()}
            disabled={saving || !dirty}
            className="qbtn-primary h-7 px-3 text-xs disabled:opacity-50"
          >
            {saving ? <Loader2 className="mr-1 h-3.5 w-3.5 animate-spin" /> : null}
            Save rules
          </button>
        </div>
      </div>
    </div>
  );
}

function VaultWatcherCard({ projectId }: { projectId: string }): JSX.Element {
  const currentProject = useApp((s) => s.currentProject);
  const updateCurrentProject = useApp((s) => s.updateCurrentProject);
  const vaultPath = currentProject?.vault_path ?? null;

  const [status, setStatus] = useState<WatchStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const pollRef = useRef<number | null>(null);

  const refresh = useCallback(async () => {
    try {
      const s = await ipc.canonWatchStatus(projectId);
      setStatus(s);
    } catch (e) {
      setErr(errToString(e));
    }
  }, [projectId]);

  // Poll for status while watching is active. 1.5s feels live without
  // hammering the IPC channel.
  useEffect(() => {
    void refresh();
    if (status) {
      pollRef.current = window.setInterval(() => {
        void refresh();
      }, 1500);
      return () => {
        if (pollRef.current !== null) {
          window.clearInterval(pollRef.current);
          pollRef.current = null;
        }
      };
    }
    return undefined;
  }, [refresh, status]);

  const pickVault = useCallback(async () => {
    setErr(null);
    try {
      const picked = await openDialog({ directory: true, multiple: false });
      if (typeof picked !== "string") return;
      await updateCurrentProject({ vault_path: picked });
    } catch (e) {
      setErr(errToString(e));
    }
  }, [updateCurrentProject]);

  const clearVault = useCallback(async () => {
    setErr(null);
    try {
      if (status) {
        await ipc.canonWatchStop(projectId);
        setStatus(null);
      }
      await updateCurrentProject({ vault_path: null });
    } catch (e) {
      setErr(errToString(e));
    }
  }, [projectId, status, updateCurrentProject]);

  const start = useCallback(async () => {
    if (!vaultPath) return;
    setBusy(true);
    setErr(null);
    try {
      const s = await ipc.canonWatchStart(projectId, vaultPath);
      setStatus(s);
    } catch (e) {
      setErr(errToString(e));
    } finally {
      setBusy(false);
    }
  }, [projectId, vaultPath]);

  const stop = useCallback(async () => {
    setBusy(true);
    setErr(null);
    try {
      await ipc.canonWatchStop(projectId);
      setStatus(null);
    } catch (e) {
      setErr(errToString(e));
    } finally {
      setBusy(false);
    }
  }, [projectId]);

  return (
    <div className="mt-3 flex flex-col gap-3 rounded-md border border-line-subtle bg-surface-subtle p-4">
      <div className="flex items-center gap-3">
        <FolderOpen className="h-4 w-4 shrink-0 text-ink-faint" />
        <div className="min-w-0 flex-1">
          {vaultPath ? (
            <div className="truncate text-sm text-ink">{vaultPath}</div>
          ) : (
            <div className="text-sm italic text-ink-faint">No vault directory set</div>
          )}
        </div>
        <button
          type="button"
          className="qbtn-ghost text-xs"
          onClick={() => void pickVault()}
        >
          {vaultPath ? "Change…" : "Pick vault…"}
        </button>
        {vaultPath && (
          <button
            type="button"
            className="qbtn-ghost text-xs"
            onClick={() => void clearVault()}
          >
            Clear
          </button>
        )}
      </div>

      {err && <ErrorBanner message={err} onDismiss={() => setErr(null)} />}

      {vaultPath && (
        <div className="flex items-center gap-3 border-t border-line-subtle pt-3">
          {status ? (
            <button
              type="button"
              className="qbtn inline-flex items-center gap-2"
              disabled={busy}
              onClick={() => void stop()}
            >
              {busy ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <EyeOff className="h-4 w-4" />
              )}
              Stop watching
            </button>
          ) : (
            <button
              type="button"
              className="qbtn-primary inline-flex items-center gap-2"
              disabled={busy}
              onClick={() => void start()}
            >
              {busy ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Eye className="h-4 w-4" />
              )}
              Start watching
            </button>
          )}
          <WatchStatusInline status={status} />
        </div>
      )}
    </div>
  );
}

function WatchStatusInline({ status }: { status: WatchStatus | null }): JSX.Element {
  if (!status) {
    return (
      <span className="text-xs text-ink-faint">
        Idle — manual ingest only until you start watching.
      </span>
    );
  }
  return (
    <div className="flex min-w-0 flex-1 flex-col gap-0.5 text-xs">
      <div className="flex items-center gap-2 text-ink-muted">
        <span className="inline-block h-2 w-2 rounded-full bg-emerald-500" />
        <span>
          Watching · {status.events_received} event
          {status.events_received === 1 ? "" : "s"} · {status.files_reingested}{" "}
          re-ingested
        </span>
      </div>
      {status.last_event_at && (
        <div className="truncate text-ink-faint">
          Last change {formatRelative(status.last_event_at)}
          {status.last_event_path && <span> · {basename(status.last_event_path)}</span>}
        </div>
      )}
      {status.last_error && (
        <div className="truncate text-rose-600 dark:text-rose-400">
          {status.last_error}
        </div>
      )}
    </div>
  );
}

function formatRelative(iso: string): string {
  const then = new Date(iso).getTime();
  const now = Date.now();
  const sec = Math.max(0, Math.round((now - then) / 1000));
  if (sec < 5) return "just now";
  if (sec < 60) return `${sec}s ago`;
  const min = Math.round(sec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.round(min / 60);
  return `${hr}h ago`;
}

function basename(p: string): string {
  return p.split("/").pop() ?? p;
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
