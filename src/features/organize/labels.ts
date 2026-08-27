/**
 * Pure naming/labeling rules for saved views, collections and similarity
 * groups. No DOM, no IPC — unit-tested so the strings the user sees are
 * deterministic.
 */

/** Max length for user-entered names (views, collections). */
export const NAME_MAX = 60;

/**
 * Trim + guard a user-entered name. Returns null when the result would be
 * empty (blank/overlong-after-trim); otherwise returns the cleaned name.
 * (Overlong input is kept as trimmed — the backend does not truncate, so we
 * validate length up front and refuse overlong names.)
 */
export function cleanName(raw: string): string | null {
  const t = raw.trim();
  if (t.length === 0) return null;
  if (t.length > NAME_MAX) return null;
  return t;
}

/** Human label for one similarity group card. */
export function groupLabel(groupType: string, count: number): string {
  if (groupType === "burst") {
    return count === 1 ? "burst · 1" : `burst · ${count}`;
  }
  if (groupType === "face") {
    return count === 2 ? "2 matching face appearances" : `${count} matching face appearances`;
  }
  return count === 2 ? "2 similar" : `${count} similar photographs`;
}
