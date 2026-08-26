import { describe, it, expect } from "vitest";
import { SHORTCUTS, shortcutFor, isTypingTarget } from "@/features/shortcuts";

const base = { ctrlKey: false, metaKey: false, altKey: false, shiftKey: false };

describe("shortcutFor", () => {
  it("maps ⌘/Ctrl+O to open-folder on both platforms", () => {
    expect(shortcutFor({ ...base, key: "o", ctrlKey: true })).toEqual({ kind: "open-folder" });
    expect(shortcutFor({ ...base, key: "O", metaKey: true })).toEqual({ kind: "open-folder" });
  });

  it("maps bare digits 1–7 to the fixed view order", () => {
    const expected = [
      "home",
      "library",
      "dashboard",
      "sessions",
      "collections",
      "saved-views",
      "settings",
    ] as const;
    for (let i = 0; i < expected.length; i++) {
      expect(shortcutFor({ ...base, key: String(i + 1) })).toEqual({
        kind: "view",
        view: expected[i],
      });
    }
  });

  it("does not claim anything it does not mean", () => {
    expect(shortcutFor({ ...base, key: "o" })).toBeNull();
    expect(shortcutFor({ ...base, key: "0" })).toBeNull();
    expect(shortcutFor({ ...base, key: "8" })).toBeNull();
    expect(shortcutFor({ ...base, key: "o", ctrlKey: true, shiftKey: true })).toBeNull();
    expect(shortcutFor({ ...base, key: "1", altKey: true })).toBeNull();
    expect(shortcutFor({ ...base, key: "Escape" })).toBeNull();
  });
});

describe("catalog", () => {
  it("covers exactly the actions the mapper can emit (global) plus the documented viewer keys", () => {
    const globalIds = SHORTCUTS.filter((s) => s.scope === "global").map((s) => s.id);
    expect(globalIds).toContain("open-folder");
    for (const id of [
      "view-home",
      "view-library",
      "view-dashboard",
      "view-sessions",
      "view-collections",
      "view-saved-views",
      "view-settings",
    ]) {
      expect(globalIds).toContain(id);
    }
    expect(SHORTCUTS.filter((s) => s.scope === "viewer").map((s) => s.id)).toEqual([
      "viewer-close",
      "viewer-prev",
      "viewer-next",
    ]);
    // Every entry has displayable text.
    for (const s of SHORTCUTS) {
      expect(s.keys.length).toBeGreaterThan(0);
      expect(s.action.length).toBeGreaterThan(0);
    }
  });
});

describe("isTypingTarget", () => {
  it("is true for editable targets, false otherwise", () => {
    expect(isTypingTarget({ tagName: "INPUT" })).toBe(true);
    expect(isTypingTarget({ tagName: "TEXTAREA" })).toBe(true);
    expect(isTypingTarget({ tagName: "DIV", isContentEditable: true })).toBe(true);
    expect(isTypingTarget({ tagName: "DIV" })).toBe(false);
    expect(isTypingTarget({ tagName: "BUTTON" })).toBe(false);
    expect(isTypingTarget(null)).toBe(false);
  });
});
