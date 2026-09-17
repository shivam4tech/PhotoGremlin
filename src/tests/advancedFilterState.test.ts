import { describe, expect, it } from "vitest";
import {
  advancedFilterWorkspaceReducer,
  createAdvancedFilterWorkspaceState,
  removeDraftCondition,
  upsertDraftCondition,
} from "@/features/library/advancedFilterState";
import type { FilterCondition } from "@/types/api";

const cameraCondition: FilterCondition = {
  field: "camera_make",
  operator: "=",
  value: "Fujifilm",
};

const paletteCondition: FilterCondition = {
  field: "palette_color",
  operator: "in",
  value: ["yellow", "green"],
};

describe("advanced filter workspace state", () => {
  it("keeps independent applied and draft snapshots", () => {
    const initial = [cameraCondition, paletteCondition];
    const state = createAdvancedFilterWorkspaceState(initial);

    expect(state.appliedFilters).toEqual(initial);
    expect(state.draftFilters).toEqual(initial);
    expect(state.appliedFilters).not.toBe(initial);
    expect(state.draftFilters).not.toBe(state.appliedFilters);

    (state.draftFilters[1].value as string[]).push("blue");
    expect(state.appliedFilters[1].value).toEqual(["yellow", "green"]);
    expect(initial[1].value).toEqual(["yellow", "green"]);
  });

  it("adds a new field and replaces an existing field without duplicates", () => {
    const added = upsertDraftCondition([cameraCondition], paletteCondition);
    expect(added).toEqual([cameraCondition, paletteCondition]);

    const replacement: FilterCondition = {
      ...cameraCondition,
      value: "Nikon",
    };
    const replaced = upsertDraftCondition(added, replacement);
    expect(replaced).toEqual([replacement, paletteCondition]);
    expect(replaced.filter(({ field }) => field === "camera_make")).toHaveLength(
      1,
    );
  });

  it("updates and removes exact staged conditions by index", () => {
    const exposure: FilterCondition = {
      field: "exposure_time",
      operator: "between",
      value: [0.001, 0.01],
    };
    const updatedExposure: FilterCondition = {
      ...exposure,
      value: [0.002, 0.02],
    };
    const draft = [cameraCondition, exposure, paletteCondition];

    expect(upsertDraftCondition(draft, updatedExposure, 1)).toEqual([
      cameraCondition,
      updatedExposure,
      paletteCondition,
    ]);
    expect(removeDraftCondition(draft, 1)).toEqual([
      cameraCondition,
      paletteCondition,
    ]);
    expect(removeDraftCondition(draft, 99)).toBe(draft);
  });

  it("commits or cancels an editor without mutating the applied snapshot", () => {
    const initial = [cameraCondition];
    let state = createAdvancedFilterWorkspaceState(initial);

    state = advancedFilterWorkspaceReducer(state, {
      type: "begin-add",
      field: paletteCondition.field,
    });
    state = advancedFilterWorkspaceReducer(state, {
      type: "update-candidate",
      candidate: paletteCondition,
    });
    state = advancedFilterWorkspaceReducer(state, { type: "cancel-editor" });
    expect(state.draftFilters).toEqual(initial);

    state = advancedFilterWorkspaceReducer(state, {
      type: "begin-add",
      field: paletteCondition.field,
    });
    state = advancedFilterWorkspaceReducer(state, {
      type: "update-candidate",
      candidate: paletteCondition,
    });
    state = advancedFilterWorkspaceReducer(state, { type: "commit-editor" });

    expect(state.draftFilters).toEqual([cameraCondition, paletteCondition]);
    expect(state.appliedFilters).toEqual(initial);
    expect(state.currentEditor).toEqual({ kind: "idle" });
  });

  it("keeps a typed candidate intact when Search opens above it", () => {
    let state = createAdvancedFilterWorkspaceState([cameraCondition]);
    state = advancedFilterWorkspaceReducer(state, { type: "begin-edit", index: 0 });
    const editing = state.currentEditor;

    state = advancedFilterWorkspaceReducer(state, { type: "open-picker" });

    expect(state.currentEditor).toEqual(editing);
    expect(state.draftFilters).toEqual([cameraCondition]);
  });

  it("clears temporary editing on category changes while preserving staged filters", () => {
    let state = createAdvancedFilterWorkspaceState([]);
    state = advancedFilterWorkspaceReducer(state, { type: "begin-add", field: "monochrome" });
    expect(state.currentEditor).toMatchObject({ kind: "editing", category: "Appearance" });
    state = advancedFilterWorkspaceReducer(state, { type: "change-category", category: "Camera & date" });
    expect(state.currentEditor).toEqual({ kind: "idle" });
    state = advancedFilterWorkspaceReducer(state, { type: "commit-editor" });
    expect(state.draftFilters).toEqual([]);

    state = advancedFilterWorkspaceReducer(state, { type: "begin-add", field: "monochrome" });
    state = advancedFilterWorkspaceReducer(state, {
      type: "update-candidate",
      candidate: { field: "monochrome", operator: "=", value: true },
    });
    state = advancedFilterWorkspaceReducer(state, { type: "commit-editor" });
    state = advancedFilterWorkspaceReducer(state, { type: "change-category", category: "Camera & date" });
    expect(state.draftFilters).toEqual([{ field: "monochrome", operator: "=", value: true }]);
    expect(state.currentEditor).toEqual({ kind: "idle" });
  });

  it("opens a staged filter in its owning category and rejects a mismatched candidate", () => {
    let state = createAdvancedFilterWorkspaceState([cameraCondition]);
    state = advancedFilterWorkspaceReducer(state, { type: "begin-edit", index: 0 });
    expect(state.activeCategory).toBe("Camera & date");
    expect(state.currentEditor).toMatchObject({ kind: "editing", category: "Camera & date" });
    state = advancedFilterWorkspaceReducer(state, { type: "update-candidate", candidate: paletteCondition });
    state = advancedFilterWorkspaceReducer(state, { type: "commit-editor" });
    expect(state.draftFilters).toEqual([cameraCondition]);
  });
});
