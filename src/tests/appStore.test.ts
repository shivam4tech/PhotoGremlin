import { describe, it, expect, beforeEach } from "vitest";
import { useAppStore, VIEW_META } from "@/stores/appStore";
import { EMPTY_FILTER } from "@/types/api";
import type { Filter } from "@/types/api";

describe("appStore", () => {
  beforeEach(() => {
    useAppStore.setState({
      view: "library",
      appInfo: null,
      dbStatus: null,
      activeFolder: null,
      scanning: false,
      analyzing: false,
      progress: null,
      scanSummary: null,
      notice: null,
      error: null,
    });
  });

  it("starts on the library view", () => {
    expect(useAppStore.getState().view).toBe("library");
  });

  it("switches views and exposes metadata for every view", () => {
    const { setView, view } = useAppStore.getState();
    for (const id of ["dashboard", "sessions", "collections", "saved-views", "settings"] as const) {
      setView(id);
      expect(useAppStore.getState().view).toBe(id);
      expect(VIEW_META[id].label.length).toBeGreaterThan(0);
    }
    expect(view).toBe("library");
  });

  it("tracks progress payloads", () => {
    useAppStore.getState().setProgress({ total: 100, done: 42, stage: "scan", current: "a.jpg" });
    const p = useAppStore.getState().progress;
    expect(p?.done).toBe(42);
    expect(p?.total).toBe(100);
  });
});

describe("filter shape", () => {
  it("empty filter serializes round-trip stably", () => {
    const json = JSON.stringify(EMPTY_FILTER);
    const parsed = JSON.parse(json) as Filter;
    expect(parsed.operator).toBe("AND");
    expect(parsed.conditions).toEqual([]);
  });
});
