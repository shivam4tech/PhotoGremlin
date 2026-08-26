/**
 * Global keyboard shortcuts (Sprint 10). Pure and testable: `shortcutFor`
 * maps a DOM-ish key event to an action; `SHORTCUTS` is the display catalog
 * the Settings card renders. The viewer keeps its own listener for
 * Esc/arrows (documented in the same catalog, scope "viewer").
 */
import type { ViewId } from "@/types/api";

export type ShortcutAction =
  | { kind: "open-folder" }
  | { kind: "view"; view: ViewId };

export interface ShortcutDef {
  id: string;
  /** Display form, platform-neutral (⌘ on macOS, Ctrl elsewhere). */
  keys: string;
  action: string;
  scope: "global" | "viewer";
}

/** Display order on the Settings card: global first, then viewer. */
export const SHORTCUTS: ShortcutDef[] = [
  { id: "open-folder", keys: "⌘ / Ctrl + O", action: "Open a photo folder", scope: "global" },
  { id: "view-home", keys: "1", action: "Go to Home", scope: "global" },
  { id: "view-library", keys: "2", action: "Go to Library", scope: "global" },
  { id: "view-dashboard", keys: "3", action: "Go to Dashboard", scope: "global" },
  { id: "view-sessions", keys: "4", action: "Go to Sessions", scope: "global" },
  { id: "view-collections", keys: "5", action: "Go to Collections", scope: "global" },
  { id: "view-saved-views", keys: "6", action: "Go to Saved Views", scope: "global" },
  { id: "view-settings", keys: "7", action: "Go to Settings", scope: "global" },
  { id: "viewer-close", keys: "Esc", action: "Close the photo viewer", scope: "viewer" },
  { id: "viewer-prev", keys: "←", action: "Previous photograph", scope: "viewer" },
  { id: "viewer-next", keys: "→", action: "Next photograph", scope: "viewer" },
];

const VIEW_BY_DIGIT: Record<string, ViewId> = {
  "1": "home",
  "2": "library",
  "3": "dashboard",
  "4": "sessions",
  "5": "collections",
  "6": "saved-views",
  "7": "settings",
};

interface KeyEventLike {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
}

/**
 * Map a key event to a global shortcut action, or null. Deliberately
 * conservative: never fires while a modifier combination is in use for
 * browser/app defaults the user may expect (e.g. ⌘/Ctrl+S) — we only claim
 * ⌘/Ctrl+O and bare digits 1–6 (no other modifiers).
 */
export function shortcutFor(e: KeyEventLike): ShortcutAction | null {
  const mod = e.ctrlKey || e.metaKey;
  if (mod && !e.altKey && !e.shiftKey && (e.key === "o" || e.key === "O")) {
    return { kind: "open-folder" };
  }
  if (!mod && !e.altKey && !e.shiftKey) {
    const view = VIEW_BY_DIGIT[e.key];
    if (view) return { kind: "view", view };
  }
  return null;
}

/**
 * Minimal shape of a DOM event target (keeps this module DOM-free and
 * unit-testable without a browser environment; HTMLElement satisfies it).
 */
export interface EditableLike {
  tagName: string;
  isContentEditable?: boolean;
}

/** True while typing — shortcuts must not fire from inputs. */
export function isTypingTarget(t: EditableLike | null): boolean {
  if (!t) return false;
  const tag = t.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || t.isContentEditable === true;
}
