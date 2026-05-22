import { cn } from "@/lib/cn";

/**
 * A slim, always-draggable header strip used on screens that don't
 * render the full TitleBar (BootSplash, BootError, ProjectPicker).
 *
 * The window has `decorations: true` + `titleBarStyle: "Overlay"` +
 * `hiddenTitle: true`, so macOS shows the traffic-light buttons but no
 * native title bar. Without this strip, those early screens have no
 * region the user can grab to move the window.
 */
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
