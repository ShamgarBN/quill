import { useCallback, useEffect, useState } from "react";
import { ViewHeader } from "@/routes/Manuscript";
import { useApp } from "@/stores/app";
import * as ipc from "@/lib/ipc";
import type { AuditEntry, FontPreference, GenerationMode, ProviderId } from "@/types";
import { cn } from "@/lib/cn";
import {
  AlertCircle,
  CheckCircle2,
  ExternalLink,
  KeyRound,
  Loader2,
} from "lucide-react";

const PROVIDER_INFO: Record<
  ProviderId,
  {
    label: string;
    secretKey: string;
    docsUrl: string;
    note: string;
    canEmbed: boolean;
  }
> = {
  gemini: {
    label: "Google Gemini",
    secretKey: "QUILL_GEMINI_API_KEY",
    docsUrl: "https://aistudio.google.com/app/apikey",
    note: "Free tier trains on inputs. Switch to a paid tier or self-hosted before publishing.",
    canEmbed: true,
  },
  groq: {
    label: "Groq (Llama 3.3 70B)",
    secretKey: "QUILL_GROQ_API_KEY",
    docsUrl: "https://console.groq.com/keys",
    note: "Drafting fallback. No embeddings endpoint — use Gemini or Mock for embeddings.",
    canEmbed: false,
  },
  mock: {
    label: "Mock (offline, deterministic)",
    secretKey: "",
    docsUrl: "",
    note: "Echoes prompts and produces hash-based vectors. Useful for development. No real generation.",
    canEmbed: true,
  },
};

export function SettingsView(): JSX.Element {
  const settings = useApp((s) => s.settings);
  const updateSettings = useApp((s) => s.updateSettings);

  const [appInfoState, setAppInfoState] = useState<{
    version: string;
    data_dir: string;
    phase: string;
  } | null>(null);

  useEffect(() => {
    ipc
      .appInfo()
      .then(setAppInfoState)
      .catch(() => undefined);
  }, []);

  if (!settings) {
    return (
      <div className="flex h-full flex-col">
        <ViewHeader title="Settings" />
        <div className="flex-1" />
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <ViewHeader title="Settings" subtitle="Phase 1 — canon + LLM" />
      <div className="flex-1 overflow-auto p-6">
        <div className="mx-auto max-w-2xl space-y-6">
          <Section title="Appearance">
            <Row label="Prose font" hint="Used in the writing pane (Phase 5+)">
              <SegControl
                value={settings.prose_font}
                options={[
                  { id: "charter", label: "Charter (serif)" },
                  { id: "jetbrains-mono", label: "JetBrains Mono" },
                ]}
                onChange={(prose_font: FontPreference) =>
                  void updateSettings({ prose_font })
                }
              />
            </Row>
          </Section>

          <Section title="Drafting">
            <Row
              label="Default generation mode"
              hint="Hot-switchable while writing (⌘⇧S / ⌘⇧P / Tab)"
            >
              <SegControl
                value={settings.default_generation_mode}
                options={[
                  { id: "scene", label: "Scene" },
                  { id: "paragraph", label: "Paragraph" },
                  { id: "sentence", label: "Sentence" },
                ]}
                onChange={(mode: GenerationMode) =>
                  void updateSettings({ default_generation_mode: mode })
                }
              />
            </Row>
          </Section>

          <ProviderSection />

          <Section title="Privacy">
            <Row
              label="Show 'what gets sent' preview"
              hint="Confirm the payload before any cloud LLM call"
            >
              <Toggle
                checked={settings.show_what_gets_sent}
                onChange={(show_what_gets_sent) =>
                  void updateSettings({ show_what_gets_sent })
                }
              />
            </Row>
            <Row label="Free-tier disclosure" hint="See docs/PRIVACY.md">
              <span
                className={cn(
                  "qbadge",
                  settings.privacy_acknowledged_at && "qbadge-accent",
                )}
              >
                {settings.privacy_acknowledged_at
                  ? "Acknowledged"
                  : "Not yet acknowledged"}
              </span>
            </Row>
            {!settings.privacy_acknowledged_at && (
              <div className="px-4 pb-3">
                <button
                  type="button"
                  className="qbtn-outline"
                  onClick={() =>
                    void updateSettings({
                      privacy_acknowledged_at: new Date().toISOString(),
                    })
                  }
                >
                  I understand the free-tier tradeoffs
                </button>
              </div>
            )}
          </Section>

          <AuditLogSection />

          <Section title="About">
            <Row label="Version">
              <span className="text-sm text-ink-muted">
                {appInfoState?.version ?? "—"}
              </span>
            </Row>
            <Row label="Phase">
              <span className="text-sm text-ink-muted">
                {appInfoState?.phase ?? "—"}
              </span>
            </Row>
            <Row label="Data directory">
              <code className="font-mono text-xs text-ink-subtle">
                {appInfoState?.data_dir ?? "—"}
              </code>
            </Row>
          </Section>
        </div>
      </div>
    </div>
  );
}

function ProviderSection(): JSX.Element {
  const settings = useApp((s) => s.settings);
  const updateSettings = useApp((s) => s.updateSettings);
  if (!settings) return <></>;

  return (
    <Section title="LLM providers">
      <div className="px-4 py-3 text-xs text-ink-faint">
        Selecting a non-Mock provider sends data to that company's servers when you
        generate or embed. Read{" "}
        <a
          className="text-accent underline"
          href="docs/PRIVACY.md"
          target="_blank"
          rel="noreferrer noopener"
        >
          docs/PRIVACY.md
        </a>{" "}
        first. The Mock provider is offline and free of side effects — keep it selected
        during development.
      </div>
      <Row label="Chat provider" hint="Used for drafting, critique, and revision">
        <SegControl
          value={settings.chat_provider}
          options={[
            { id: "mock", label: "Mock" },
            { id: "gemini", label: "Gemini" },
            { id: "groq", label: "Groq" },
          ]}
          onChange={(chat_provider: ProviderId) =>
            void updateSettings({ chat_provider })
          }
        />
      </Row>
      <Row
        label="Embeddings provider"
        hint="Used for canon retrieval (Mock is term-bag, Gemini is real semantic)"
      >
        <SegControl
          value={settings.embedding_provider}
          options={[
            { id: "mock", label: "Mock" },
            { id: "gemini", label: "Gemini" },
          ]}
          onChange={(embedding_provider: ProviderId) =>
            void updateSettings({ embedding_provider })
          }
        />
      </Row>

      <ProviderKeyRow provider="gemini" />
      <ProviderKeyRow provider="groq" />
    </Section>
  );
}

function ProviderKeyRow({ provider }: { provider: ProviderId }): JSX.Element {
  const info = PROVIDER_INFO[provider];
  const [hasKey, setHasKey] = useState<boolean | null>(null);
  const [draftKey, setDraftKey] = useState("");
  const [showInput, setShowInput] = useState(false);
  const [saving, setSaving] = useState(false);
  const [pingState, setPingState] = useState<
    | { kind: "idle" }
    | { kind: "pending" }
    | { kind: "ok"; reply: string }
    | { kind: "err"; message: string }
  >({ kind: "idle" });

  const refresh = useCallback(async () => {
    if (!info.secretKey) return;
    const status = await ipc.llmProviderStatus(provider);
    setHasKey(status.has_key);
  }, [provider, info.secretKey]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  if (provider === "mock") return <></>;

  const onSave = async (): Promise<void> => {
    if (!draftKey.trim()) return;
    setSaving(true);
    try {
      await ipc.secretSet(info.secretKey, draftKey.trim());
      setDraftKey("");
      setShowInput(false);
      await refresh();
    } finally {
      setSaving(false);
    }
  };

  const onPing = async (): Promise<void> => {
    setPingState({ kind: "pending" });
    try {
      const reply = await ipc.llmPing(provider);
      setPingState({ kind: "ok", reply });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setPingState({ kind: "err", message: msg });
    }
  };

  return (
    <div className="px-4 py-3">
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 text-sm text-ink">
            <KeyRound className="h-4 w-4 text-ink-faint" />
            {info.label}
            {hasKey && (
              <span className="qbadge qbadge-accent">
                <CheckCircle2 className="h-3 w-3" />
                Key set
              </span>
            )}
          </div>
          <div className="mt-0.5 text-xs text-ink-faint">{info.note}</div>
          <a
            href={info.docsUrl}
            target="_blank"
            rel="noreferrer noopener"
            className="mt-1 inline-flex items-center gap-1 text-xs text-accent hover:underline"
          >
            Get a key <ExternalLink className="h-3 w-3" />
          </a>
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            className="qbtn-outline"
            onClick={() => setShowInput((v) => !v)}
          >
            {hasKey ? "Replace" : "Set"} key
          </button>
          {hasKey && (
            <button
              type="button"
              className="qbtn-ghost"
              onClick={() => void onPing()}
              disabled={pingState.kind === "pending"}
            >
              {pingState.kind === "pending" ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                "Ping"
              )}
            </button>
          )}
        </div>
      </div>

      {showInput && (
        <div className="mt-3 flex gap-2">
          <input
            type="password"
            className="qinput flex-1 font-mono text-xs"
            placeholder={`paste ${info.label} API key`}
            value={draftKey}
            onChange={(e) => setDraftKey(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void onSave();
            }}
          />
          <button
            type="button"
            className="qbtn-primary"
            onClick={() => void onSave()}
            disabled={saving || !draftKey.trim()}
          >
            {saving ? "Saving…" : "Save"}
          </button>
        </div>
      )}

      {pingState.kind === "ok" && (
        <div className="mt-3 rounded-md border border-emerald-300 bg-emerald-50 px-3 py-2 text-xs text-emerald-900 dark:border-emerald-700/40 dark:bg-emerald-900/20 dark:text-emerald-200">
          <div className="flex items-center gap-2 font-medium">
            <CheckCircle2 className="h-3.5 w-3.5" /> Provider responded
          </div>
          <div className="mt-1 break-words italic">"{pingState.reply}"</div>
        </div>
      )}
      {pingState.kind === "err" && (
        <div className="mt-3 rounded-md border border-rose-300 bg-rose-50 px-3 py-2 text-xs text-rose-900 dark:border-rose-700/40 dark:bg-rose-900/20 dark:text-rose-200">
          <div className="flex items-center gap-2 font-medium">
            <AlertCircle className="h-3.5 w-3.5" /> Ping failed
          </div>
          <div className="mt-1 break-words font-mono">{pingState.message}</div>
        </div>
      )}
    </div>
  );
}

function AuditLogSection(): JSX.Element {
  const [entries, setEntries] = useState<AuditEntry[]>([]);
  const [path, setPath] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const [tail, p] = await Promise.all([ipc.auditTail(20), ipc.auditPath()]);
      setEntries(tail.reverse()); // newest first
      setPath(p);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return (
    <Section title="Audit log">
      <div className="px-4 py-3 text-xs text-ink-faint">
        Append-only log of every cloud LLM call. We log the categories of content sent —
        never the content itself.
      </div>
      {entries.length === 0 ? (
        <div className="px-4 pb-3 text-sm text-ink-faint">
          {loading ? "Loading…" : "No cloud calls have been made yet."}
        </div>
      ) : (
        <ul className="divide-y divide-line-subtle">
          {entries.map((e, i) => (
            <li key={i} className="px-4 py-2 text-xs">
              <div className="flex items-center justify-between gap-3">
                <div className="font-medium text-ink">
                  {e.success ? (
                    <CheckCircle2 className="mr-1 inline h-3 w-3 text-emerald-600 dark:text-emerald-400" />
                  ) : (
                    <AlertCircle className="mr-1 inline h-3 w-3 text-rose-600 dark:text-rose-400" />
                  )}
                  {e.operation}
                </div>
                <div className="text-ink-faint tabular-nums">
                  {new Date(e.timestamp).toLocaleString()}
                </div>
              </div>
              <div className="mt-1 text-ink-muted">
                {e.provider} ({e.model}) · in: {e.tokens_in} · out: {e.tokens_out}
              </div>
              {e.included.length > 0 && (
                <div className="mt-1 text-ink-faint">sent: {e.included.join(", ")}</div>
              )}
              {e.error && (
                <div className="mt-1 break-words font-mono text-rose-700 dark:text-rose-400">
                  {e.error}
                </div>
              )}
            </li>
          ))}
        </ul>
      )}
      <div className="px-4 py-3">
        <code className="font-mono text-xs text-ink-subtle">{path ?? "—"}</code>
      </div>
    </Section>
  );
}

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}): JSX.Element {
  return (
    <section>
      <h2 className="mb-2 text-xs font-semibold uppercase tracking-wider text-ink-muted">
        {title}
      </h2>
      <div className="qcard divide-y divide-line-subtle">{children}</div>
    </section>
  );
}

function Row({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}): JSX.Element {
  return (
    <div className="flex items-start justify-between gap-6 px-4 py-3">
      <div className="min-w-0 flex-1">
        <div className="text-sm text-ink">{label}</div>
        {hint && <div className="mt-0.5 text-xs text-ink-faint">{hint}</div>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

interface SegOption<T extends string> {
  id: T;
  label: string;
}

function SegControl<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: SegOption<T>[];
  onChange: (v: T) => void;
}): JSX.Element {
  return (
    <div className="flex items-center gap-0.5 rounded-md border border-line-subtle bg-surface-muted p-0.5">
      {options.map((o) => {
        const active = value === o.id;
        return (
          <button
            key={o.id}
            type="button"
            onClick={() => onChange(o.id)}
            className={cn(
              "rounded px-2.5 py-1 text-xs transition-colors",
              active
                ? "bg-surface text-ink shadow-soft"
                : "text-ink-muted hover:text-ink",
            )}
          >
            {o.label}
          </button>
        );
      })}
    </div>
  );
}

function Toggle({
  checked,
  onChange,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
}): JSX.Element {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      onClick={() => onChange(!checked)}
      className={cn(
        "relative h-5 w-9 rounded-full border transition-colors duration-150",
        checked ? "border-accent bg-accent" : "border-line bg-surface-muted",
      )}
    >
      <span
        className={cn(
          "absolute top-0.5 h-3.5 w-3.5 rounded-full bg-surface-overlay shadow-soft transition-transform duration-150",
          checked ? "translate-x-4" : "translate-x-0.5",
        )}
      />
    </button>
  );
}
