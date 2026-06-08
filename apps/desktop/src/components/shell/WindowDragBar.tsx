import { getCurrentWindow } from "@tauri-apps/api/window";
import { cn } from "@/lib/cn";

/**
 * A slim, always-draggable header strip used on screens that don't
 * render the full TitleBar (BootSplash, BootError, ProjectPicker).
 *
 * The window has `decorations: true` + `titleBarStyle: "Overlay"` +
 * `hiddenTitle: true`, so macOS shows the traffic-light buttons but no
 * native title bar. Without this strip, those early screens have no
 * region the user can grab to move the window.
 *
 * Drag is implemented via explicit `startDragging()` on mousedown — the
 * `data-tauri-drag-region` attribute alone is unreliable on macOS
 * WKWebView. The attributes are kept for resiliency.
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
export function WindowDragBar({
  label,
  className,
}: {
  label?: string;
  className?: string;
}): JSX.Element {
  return (
    <div
      data-tauri-drag-region
      onMouseDown={dragOnMouseDown}
      className={cn(
        "app-chrome flex h-9 shrink-0 items-center justify-between",
        "border-b border-line-subtle bg-surface-subtle px-3",
        className,
      )}
    >
      {/* Left: macOS traffic-light spacer (matches TitleBar layout) */}
      <div className="w-16" data-tauri-drag-region />

      {/* Center: app name / status */}
      <div
        data-tauri-drag-region
        className="flex items-center gap-2 text-xs font-medium tracking-wide text-ink-muted"
      >
        <span className="h-1.5 w-1.5 rounded-full bg-accent" data-tauri-drag-region />
        {label ?? "Quill"}
      </div>

      {/* Right: keep symmetric with TitleBar's controls column */}
      <div className="w-16" data-tauri-drag-region />
    </div>
  );
}
