import { Sun, Moon, Monitor } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useApp } from "@/stores/app";
import type { ThemePreference } from "@/types";
import { cn } from "@/lib/cn";

/**
 * `data-tauri-drag-region` is supposed to make a node draggable, but on
 * macOS WKWebView (Tauri 2) the attribute-based path is unreliable: it
 * silently no-ops in builds where Tauri's mousedown handler isn't
 * attached early enough, or when CSP / overlay layout interferes. We
 * sidestep it by calling `startDragging()` explicitly on primary-button
 * mousedown, ignoring clicks that land on real interactive controls.
 */
const dragOnMouseDown = (e: React.MouseEvent<HTMLElement>): void => {
  if (e.button !== 0) return;
  const target = e.target as HTMLElement | null;
  if (target?.closest("button, a, input, select, textarea, [role='button']")) {
    return;
  }
  e.preventDefault();
  void getCurrentWindow().startDragging();
};

const THEMES: { id: ThemePreference; icon: typeof Sun; label: string }[] = [
  { id: "light", icon: Sun, label: "Light" },
  { id: "dark", icon: Moon, label: "Dark" },
  { id: "system", icon: Monitor, label: "System" },
];

export function TitleBar(): JSX.Element {
  const settings = useApp((s) => s.settings);
  const setTheme = useApp((s) => s.setTheme);
  const focusMode = useApp((s) => s.focusMode);
  const toggleFocus = useApp((s) => s.toggleFocus);

  return (
    <div
      data-tauri-drag-region
      onMouseDown={dragOnMouseDown}
      className={cn(
        "app-chrome flex h-9 shrink-0 items-center justify-between",
        "border-b border-line-subtle bg-surface-subtle px-3",
      )}
    >
      {/* Left: macOS traffic light spacer (Tauri renders them; keep room) */}
      <div className="w-16" />

      {/* Center: app name */}
      <div className="flex items-center gap-2 text-xs font-medium tracking-wide text-ink-muted">
        <span className="h-1.5 w-1.5 rounded-full bg-accent" />
        Quill
      </div>

      {/* Right: theme switcher + focus toggle */}
      <div className="flex items-center gap-1">
        <button
          type="button"
          onClick={toggleFocus}
          className={cn(
            "qbtn-ghost h-7 px-2 text-xs",
            focusMode && "bg-accent-subtle text-accent",
          )}
          title="Toggle focus mode (⌘.)"
        >
          Focus
        </button>
        <div className="mx-1 h-4 w-px bg-line" />
        <ThemeToggle current={settings?.theme ?? "system"} onSet={setTheme} />
      </div>
    </div>
  );
}

function ThemeToggle({
  current,
  onSet,
}: {
  current: ThemePreference;
  onSet: (t: ThemePreference) => void;
}): JSX.Element {
  return (
    <div className="flex items-center gap-0.5 rounded-md border border-line-subtle bg-surface p-0.5">
      {THEMES.map((t) => {
        const Icon = t.icon;
        const active = current === t.id;
        return (
          <button
            key={t.id}
            type="button"
            onClick={() => onSet(t.id)}
            title={t.label}
            aria-label={`Set theme to ${t.label}`}
            aria-pressed={active}
            className={cn(
              "flex h-6 w-6 items-center justify-center rounded transition-colors duration-150 ease-quill",
              active
                ? "bg-accent-subtle text-accent"
                : "text-ink-subtle hover:bg-surface-muted hover:text-ink",
            )}
          >
            <Icon className="h-3.5 w-3.5" />
          </button>
        );
      })}
    </div>
  );
}
