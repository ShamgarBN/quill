/**
 * Tauri rejects command promises with a `{ kind, message }` object — see
 * `src-tauri/src/error.rs::WireError`. The naïve `String(e)` of that
 * object yields the useless "[object Object]". Use this helper at every
 * frontend `catch` site to extract a human-readable message.
 */
export function errToString(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  if (e && typeof e === "object") {
    const obj = e as { message?: unknown; kind?: unknown };
    if (typeof obj.message === "string") return obj.message;
    try {
      return JSON.stringify(e);
    } catch {
      // fallthrough
    }
  }
  return String(e);
}
