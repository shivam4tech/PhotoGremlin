import { useEffect, useReducer, useRef, useState } from "react";
import type { FilterCondition } from "@/types/api";
import { DashboardIcon, EyeIcon, SavedViewsIcon, SettingsIcon, SunIcon } from "@/components/Icons";
import { ActiveFilterList } from "./ActiveFilterList";
import {
  advancedFilterWorkspaceReducer,
  createAdvancedFilterWorkspaceState,
  type AdvancedFilterCategory,
} from "./advancedFilterState";
import { ColorSpectrumFilter } from "./ColorSpectrumFilter";
import { FilterBar } from "./FilterBar";

const CATEGORIES = [
  { label: "Appearance", icon: SunIcon, description: "Color, light, orientation and visual characteristics." },
  { label: "Rating & review", icon: SavedViewsIcon, description: "Your ratings, labels and review decisions." },
  { label: "Image properties", icon: DashboardIcon, description: "Measured detail, exposure and image properties." },
  { label: "People & content", icon: EyeIcon, description: "Locally analyzed faces, expressions and content." },
  { label: "Camera & date", icon: SettingsIcon, description: "Camera, lens, exposure settings and capture dates." },
] as const satisfies ReadonlyArray<{ label: AdvancedFilterCategory; icon: typeof SunIcon; description: string }>;

/** A private draft: only Apply publishes conditions to the existing filter store. */
export function AdvancedFiltersDialog({ initialConditions, sessionId, disabled, loadPreviewCount, onApply, onClose }: {
  initialConditions: FilterCondition[];
  sessionId: number | null;
  disabled?: boolean;
  loadPreviewCount?: (conditions: FilterCondition[]) => Promise<number>;
  onApply: (conditions: FilterCondition[]) => void;
  onClose: () => void;
}) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const [workspace, dispatch] = useReducer(
    advancedFilterWorkspaceReducer,
    initialConditions,
    createAdvancedFilterWorkspaceState,
  );
  const draft = workspace.draftFilters;
  const setDraft = (filters: FilterCondition[]) =>
    dispatch({ type: "replace-draft", filters });
  const category = CATEGORIES.find((item) => item.label === workspace.activeCategory)!;
  const [autoFocusSearch, setAutoFocusSearch] = useState(true);
  const [preview, setPreview] = useState<{ count: number | null; loading: boolean; failed: boolean }>({
    count: null,
    loading: !!loadPreviewCount,
    failed: false,
  });
  const previewRequest = useRef(0);

  useEffect(() => {
    const dialog = dialogRef.current;
    const previousFocus = document.activeElement as HTMLElement | null;
    if (dialog && !dialog.open) dialog.showModal();
    return () => { if (dialog?.open) dialog.close(); previousFocus?.focus(); };
  }, []);

  useEffect(() => {
    if (!loadPreviewCount) return;
    const request = ++previewRequest.current;
    setPreview((current) => ({ ...current, loading: true, failed: false }));
    const timeout = window.setTimeout(() => {
      void loadPreviewCount(draft).then((count) => {
        if (previewRequest.current === request) {
          setPreview({ count, loading: false, failed: false });
        }
      }).catch(() => {
        if (previewRequest.current === request) {
          setPreview({ count: null, loading: false, failed: true });
        }
      });
    }, 180);
    return () => {
      window.clearTimeout(timeout);
      previewRequest.current += 1;
    };
  }, [draft, loadPreviewCount]);

  function applyDraft() {
    if (disabled) return;
    onApply(draft);
    onClose();
  }

  return <dialog ref={dialogRef} className="advanced-filters-dialog advanced-filters-drawer" aria-labelledby="advanced-filters-title"
    onKeyDown={(event) => {
      if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
        event.preventDefault();
        event.stopPropagation();
        applyDraft();
      }
    }}
    onCancel={(event) => { event.preventDefault(); onClose(); }}>
    <header className="advanced-filters-header">
      <h2 id="advanced-filters-title">Filters</h2>
      <button className="btn btn-ghost btn-sm" disabled={disabled || !draft.length} onClick={() => dispatch({ type: "clear-draft" })}>Clear all</button>
      <button className="btn btn-ghost" aria-label="Close advanced filters" onClick={onClose}>×</button>
    </header>
    <div className="advanced-filters-body">
      <nav className="advanced-filters-categories" aria-label="Filter categories">
        {CATEGORIES.map((item) => <button key={item.label} type="button" aria-current={category.label === item.label ? "page" : undefined}
          onClick={() => { setAutoFocusSearch(false); dispatch({ type: "change-category", category: item.label }); }}><item.icon size={18} />{item.label}</button>)}
      </nav>
      <main className="advanced-filters-main" aria-label={category.label}>
        <div className="advanced-category-heading"><h3>{category.label}</h3><p>{category.description}</p></div>
        <div className="advanced-filters-selection">
          <ActiveFilterList draft={draft} onChange={setDraft} disabled={disabled} label="Staged filters"
            onEdit={(index) => {
              if (draft[index]?.field === "palette_color") {
                setAutoFocusSearch(false);
                dispatch({ type: "change-category", category: "Appearance" });
                window.requestAnimationFrame(() => dialogRef.current?.querySelector<HTMLButtonElement>(".color-swatch")?.focus());
              } else {
                dispatch({ type: "begin-edit", index });
              }
            }}
            onRemove={(index) => dispatch({ type: "remove-filter", index })} />
        </div>
        <div key={category.label} className="advanced-category-content">
          {category.label === "Appearance" && <ColorSpectrumFilter draft={draft} onChange={setDraft} disabled={disabled} />}
          <FilterBar mode="inspector" category={category.label} draft={draft}
            onChange={setDraft} sessionId={sessionId} disabled={disabled} autoFocusSearch={autoFocusSearch}
            editorState={workspace.currentEditor.kind === "editing" && workspace.currentEditor.category !== category.label
              ? { kind: "idle" } : workspace.currentEditor}
            editorDispatch={dispatch}
            onPaletteSelect={() => {
              setAutoFocusSearch(false);
              dispatch({ type: "change-category", category: "Appearance" });
              window.requestAnimationFrame(() => dialogRef.current?.querySelector<HTMLButtonElement>(".color-swatch")?.focus());
            }} />
        </div>
      </main>
    </div>
    <footer className="advanced-filters-footer">
      <span className="advanced-filter-preview" aria-live="polite" aria-atomic="true">
        {preview.loading
          ? "Checking matches…"
          : preview.failed
            ? "Preview unavailable"
            : preview.count !== null
              ? preview.count === 0
                ? "No matching photos"
                : `${preview.count.toLocaleString()} matching photo${preview.count === 1 ? "" : "s"}`
              : draft.length
                ? `${draft.length} filter${draft.length === 1 ? "" : "s"} selected`
                : "All photos"}
      </span>
      <button className="btn" onClick={onClose}>Cancel</button>
      <button className="btn btn-primary" disabled={disabled} onClick={applyDraft}>
        Apply filters{draft.length ? ` (${draft.length})` : ""}
      </button>
    </footer>
  </dialog>;
}
