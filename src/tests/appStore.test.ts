import { describe, it, expect, beforeEach, vi, afterEach } from "vitest";
import { api } from "@/lib/ipc";
import { useAppStore, VIEW_META } from "@/stores/appStore";
import { EMPTY_FILTER } from "@/types/api";
import type { Filter } from "@/types/api";

vi.mock("@/lib/ipc", () => ({
  api: {
    dbStatus: vi.fn(async () => ({
      photo_count: 0,
      session_count: 0,
      unanalyzed: 0,
      metadata_pending: 0,
      faces_done: 0,
      schema_version: 10,
    })),
    getActiveFolder: vi.fn(async () => null),
    pickFolder: vi.fn(),
    setActiveFolder: vi.fn(async () => {}),
    updateMarks: vi.fn(async () => 1),
  },
  toErrorMessage: (e: unknown) => (e instanceof Error ? e.message : String(e)),
  onProgress: vi.fn(async () => () => {}),
}));

describe("appStore", () => {
  beforeEach(() => {
    vi.mocked(api.pickFolder).mockReset();
    vi.mocked(api.setActiveFolder).mockReset();
    vi.mocked(api.setActiveFolder).mockResolvedValue(undefined);
    vi.mocked(api.getActiveFolder).mockReset();
    vi.mocked(api.getActiveFolder).mockResolvedValue(null);
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
      theme: "dark",
      marksVersion: 0,
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
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

  it("openFolder persists the chosen folder so the next start restores it", async () => {
    vi.mocked(api.pickFolder).mockResolvedValue("/home/me/photos");
    const picked = await useAppStore.getState().openFolder();
    expect(picked).toBe("/home/me/photos");
    expect(useAppStore.getState().activeFolder).toBe("/home/me/photos");
    expect(api.setActiveFolder).toHaveBeenCalledWith("/home/me/photos");
  });

  it("openFolder does not persist when the dialog was dismissed", async () => {
    vi.mocked(api.pickFolder).mockResolvedValue(null);
    const picked = await useAppStore.getState().openFolder();
    expect(picked).toBeNull();
    expect(api.setActiveFolder).not.toHaveBeenCalled();
  });

  it("invalidates every photo surface after marks are stored", async () => {
    await useAppStore.getState().updateMarks([42], 4, null, null);
    expect(api.updateMarks).toHaveBeenCalledWith([42], 4, null, null);
    expect(useAppStore.getState().marksVersion).toBe(1);
  });
});

describe("appStore theme", () => {
  beforeEach(() => {
    useAppStore.setState({ theme: "dark" });
  });

  it("defaults to dark (the darkroom look)", () => {
    expect(useAppStore.getState().theme).toBe("dark");
  });

  it("setTheme switches state (applied + persisted by the store)", () => {
    useAppStore.getState().setTheme("light");
    expect(useAppStore.getState().theme).toBe("light");
    useAppStore.getState().setTheme("dark");
    expect(useAppStore.getState().theme).toBe("dark");
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
