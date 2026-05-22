import { useEffect, useState } from "react";
import { ViewHeader } from "@/routes/Manuscript";
import { useApp } from "@/stores/app";
import * as ipc from "@/lib/ipc";
import type { FontPreference, GenerationMode } from "@/types";
import { cn } from "@/lib/cn";

export function SettingsView(): JSX.Element {
  const settings = useApp((s) => s.settings);
  const updateSettings = useApp((s) => s.updateSettings);

  const [appInfo, setAppInfo] = useState<{
    version: string;
    data_dir: string;
    phase: string;
  } | null>(null);

  useEffect(() => {
    ipc.appInfo().then(setAppInfo).catch(() => undefined);
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
      <ViewHeader title="Settings" subtitle="Phase 0 — foundation" />
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
          </Section>

          <Section title="About">
            <Row label="Version">
              <span className="text-sm text-ink-muted">
                {appInfo?.version ?? "—"}
              </span>
            </Row>
            <Row label="Phase">
              <span className="text-sm text-ink-muted">
                {appInfo?.phase ?? "—"}
              </span>
            </Row>
            <Row label="Data directory">
              <code className="font-mono text-xs text-ink-subtle">
                {appInfo?.data_dir ?? "—"}
              </code>
            </Row>
          </Section>
        </div>
      </div>
    </div>
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
        checked
          ? "border-accent bg-accent"
          : "border-line bg-surface-muted",
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
