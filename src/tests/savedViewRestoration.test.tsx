// @vitest-environment happy-dom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Sidebar } from "@/components/Sidebar";
import { useAppStore } from "@/stores/appStore";
import type { FilterCondition, SavedView } from "@/types/api";

vi.mock("@/lib/ipc", () => ({
  api: {},
  toErrorMessage: (error: unknown) => error instanceof Error ? error.message : String(error),
}));

const RESTORED_CONDITIONS: FilterCondition[] = [
  { field: "orientation", operator: "=", value: "landscape" },
  { field: "rating", operator: ">=", value: 3 },
];

const SAVED_VIEW: SavedView = {
  id: 7,
  name: "Landscape selects",
  filter_json: JSON.stringify({ operator: "AND", conditions: RESTORED_CONDITIONS }),
  description: "Wide photographs rated three stars or higher",
  created_at: "2026-09-12T10:00:00Z",
  updated_at: "2026-09-12T10:00:00Z",
};

describe("saved-view restoration", () => {
  let container: HTMLDivElement;
  let root: ReturnType<typeof createRoot>;

  beforeEach(async () => {
    Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
    useAppStore.setState({
      view: "saved-views",
      activeFolder: null,
      recentProjects: [],
      savedViews: [SAVED_VIEW],
      filterConditions: [{ field: "monochrome", operator: "=", value: true }],
      error: null,
    });
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
    await act(async () => root.render(<Sidebar />));
  });

  afterEach(async () => {
    await act(async () => root.unmount());
    container.remove();
    vi.restoreAllMocks();
  });

  it("replaces the current filter with the saved conditions and opens Library", async () => {
    const savedViewButton = Array.from(container.querySelectorAll("button"))
      .find((button) => button.textContent?.includes(SAVED_VIEW.name));
    expect(savedViewButton).toBeDefined();

    await act(async () => savedViewButton!.click());

    expect(useAppStore.getState().filterConditions).toEqual(RESTORED_CONDITIONS);
    expect(useAppStore.getState().view).toBe("library");
    expect(savedViewButton!.getAttribute("aria-pressed")).toBe("true");
  });
});
