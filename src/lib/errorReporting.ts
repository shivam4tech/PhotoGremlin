/**
 * Error reporting helpers. Reports are sent only to the local Rust logger;
 * user-facing UI continues to show friendly messages rather than stacks.
 */
export type ClientErrorReport = {
  source: string;
  message: string;
  stack: string | null;
};

const MAX_LOG_FIELD = 16_000;

function bounded(value: string): string {
  return value.length <= MAX_LOG_FIELD
    ? value
    : `${value.slice(0, MAX_LOG_FIELD)}… [truncated]`;
}

/** Convert arbitrary browser rejection values into bounded, local-log data. */
export function clientErrorReport(source: string, value: unknown): ClientErrorReport {
  if (value instanceof Error) {
    return {
      source,
      message: bounded(value.message || value.name || "Unknown browser error"),
      stack: value.stack ? bounded(value.stack) : null,
    };
  }

  const message =
    typeof value === "string"
      ? value
      : value === null || value === undefined
        ? "Unknown browser error"
        : String(value);
  return { source, message: bounded(message), stack: null };
}

/**
 * Tauri/GTK can hand rendered text to a Pango markup path. Escape every
 * markup metacharacter, not only ampersands, so paths such as `a < b & c`
 * remain literal text and never become malformed markup.
 */
export function escapeGtkMarkupText(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&apos;");
}
