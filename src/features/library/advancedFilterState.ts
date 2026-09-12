import type { FilterCondition } from "@/types/api";

export type AdvancedFilterEditorState =
  | { kind: "idle" }
  | { kind: "picker" }
  | {
      kind: "editing";
      intent: "add";
      field: string;
      candidate: FilterCondition | null;
    }
  | {
      kind: "editing";
      intent: "edit";
      field: string;
      index: number;
      candidate: FilterCondition | null;
    };

export interface AdvancedFilterWorkspaceState {
  appliedFilters: FilterCondition[];
  draftFilters: FilterCondition[];
  currentEditor: AdvancedFilterEditorState;
}

export type AdvancedFilterWorkspaceAction =
  | { type: "replace-draft"; filters: FilterCondition[] }
  | { type: "clear-draft" }
  | { type: "open-picker" }
  | { type: "begin-add"; field: string }
  | { type: "begin-edit"; index: number }
  | { type: "update-candidate"; candidate: FilterCondition | null }
  | { type: "commit-editor" }
  | { type: "cancel-editor" }
  | { type: "remove-filter"; index: number };

function cloneConditions(conditions: FilterCondition[]): FilterCondition[] {
  return structuredClone(conditions);
}

export function createAdvancedFilterWorkspaceState(
  appliedFilters: FilterCondition[],
): AdvancedFilterWorkspaceState {
  const appliedSnapshot = cloneConditions(appliedFilters);

  return {
    appliedFilters: appliedSnapshot,
    draftFilters: cloneConditions(appliedSnapshot),
    currentEditor: { kind: "idle" },
  };
}

export function upsertDraftCondition(
  draftFilters: FilterCondition[],
  candidate: FilterCondition,
  editIndex?: number,
): FilterCondition[] {
  if (editIndex !== undefined) {
    if (editIndex < 0 || editIndex >= draftFilters.length) {
      return draftFilters;
    }

    return draftFilters.map((condition, index) =>
      index === editIndex ? structuredClone(candidate) : condition,
    );
  }

  const existingIndex = draftFilters.findIndex(
    (condition) => condition.field === candidate.field,
  );
  if (existingIndex >= 0) {
    return draftFilters.map((condition, index) =>
      index === existingIndex ? structuredClone(candidate) : condition,
    );
  }

  return [...draftFilters, structuredClone(candidate)];
}

export function removeDraftCondition(
  draftFilters: FilterCondition[],
  indexToRemove: number,
): FilterCondition[] {
  if (indexToRemove < 0 || indexToRemove >= draftFilters.length) {
    return draftFilters;
  }

  return draftFilters.filter((_, index) => index !== indexToRemove);
}

export function advancedFilterWorkspaceReducer(
  state: AdvancedFilterWorkspaceState,
  action: AdvancedFilterWorkspaceAction,
): AdvancedFilterWorkspaceState {
  switch (action.type) {
    case "replace-draft":
      return {
        ...state,
        draftFilters: cloneConditions(action.filters),
        currentEditor: { kind: "idle" },
      };
    case "clear-draft":
      return {
        ...state,
        draftFilters: [],
        currentEditor: { kind: "idle" },
      };
    case "open-picker":
      return { ...state, currentEditor: { kind: "picker" } };
    case "begin-add":
      return {
        ...state,
        currentEditor: {
          kind: "editing",
          intent: "add",
          field: action.field,
          candidate: null,
        },
      };
    case "begin-edit": {
      const condition = state.draftFilters[action.index];
      if (!condition) return state;

      return {
        ...state,
        currentEditor: {
          kind: "editing",
          intent: "edit",
          field: condition.field,
          index: action.index,
          candidate: structuredClone(condition),
        },
      };
    }
    case "update-candidate":
      if (state.currentEditor.kind !== "editing") return state;
      return {
        ...state,
        currentEditor: {
          ...state.currentEditor,
          candidate: action.candidate
            ? structuredClone(action.candidate)
            : null,
        },
      };
    case "commit-editor": {
      const editor = state.currentEditor;
      if (editor.kind !== "editing" || !editor.candidate) return state;

      return {
        ...state,
        draftFilters: upsertDraftCondition(
          state.draftFilters,
          editor.candidate,
          editor.intent === "edit" ? editor.index : undefined,
        ),
        currentEditor: { kind: "idle" },
      };
    }
    case "cancel-editor":
      return { ...state, currentEditor: { kind: "idle" } };
    case "remove-filter": {
      const draftFilters = removeDraftCondition(
        state.draftFilters,
        action.index,
      );
      if (draftFilters === state.draftFilters) return state;

      const editor = state.currentEditor;
      let currentEditor = editor;
      if (editor.kind === "editing" && editor.intent === "edit") {
        if (editor.index === action.index) {
          currentEditor = { kind: "idle" };
        } else if (editor.index > action.index) {
          currentEditor = { ...editor, index: editor.index - 1 };
        }
      }

      return { ...state, draftFilters, currentEditor };
    }
  }
}
