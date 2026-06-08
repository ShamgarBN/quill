/**
 * In-app replacement for `window.prompt()`, which is not implemented in
 * Tauri's WKWebView on macOS (returns null instantly with no UI).
 *
 * Used by Manuscript / Bible / Threads to ask for a new item's title.
 * Single text input, OK/Cancel, Enter submits, Esc cancels.
 */
import { useEffect, useRef, useState } from "react";

export function PromptDialog({
  title,
  label,
  placeholder,
  initialValue = "",
  submitLabel = "Create",
  onSubmit,
  onCancel,
}: {
  title: string;
  label?: string;
  placeholder?: string;
  initialValue?: string;
  submitLabel?: string;
  onSubmit: (value: string) => void;
  onCancel: () => void;
}): JSX.Element {
  const [value, setValue] = useState(initialValue);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === "Escape") {
        e.preventDefault();
        onCancel();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel]);

  const submit = (): void => {
    const trimmed = value.trim();
    if (!trimmed) return;
    onSubmit(trimmed);
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/50 p-8 pt-24"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onCancel();
      }}
    >
      <div className="qcard flex w-full max-w-md flex-col overflow-hidden">
        <div className="border-b border-line-subtle px-5 py-3">
          <div className="text-sm font-semibold text-ink">{title}</div>
        </div>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            submit();
          }}
          className="flex flex-col gap-3 px-5 py-4"
        >
          {label && <label className="text-xs font-medium text-ink-muted">{label}</label>}
          <input
            ref={inputRef}
            type="text"
            value={value}
            onChange={(e) => setValue(e.target.value)}
            placeholder={placeholder}
            className="qinput text-sm"
          />
          <div className="mt-1 flex items-center justify-end gap-2">
            <button type="button" className="qbtn-ghost" onClick={onCancel}>
              Cancel
            </button>
            <button
              type="submit"
              className="qbtn-primary"
              disabled={value.trim().length === 0}
            >
              {submitLabel}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
