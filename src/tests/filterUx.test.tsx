// @vitest-environment happy-dom
import { act, useState, type ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { FilterPicker } from "@/features/library/FilterPicker";
import { ActiveFilterList } from "@/features/library/ActiveFilterList";
import { QuickFilterControls } from "@/features/library/QuickFilterControls";
import { ColorSpectrumFilter } from "@/features/library/ColorSpectrumFilter";
import { AdvancedFiltersDialog } from "@/features/library/AdvancedFiltersDialog";
import { FilterBar } from "@/features/library/FilterBar";
import { api } from "@/lib/ipc";
import type { FilterCondition } from "@/types/api";

vi.mock("@/lib/ipc", () => ({ api: {
  numericFilterStats: vi.fn(async () => ({
    recorded_count: 120, missing_count: 2, minimum: 0, maximum: 100,
  })),
} }));
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
let root: Root;
let container: HTMLDivElement;
beforeEach(() => {
  vi.mocked(api.numericFilterStats).mockResolvedValue({
    recorded_count: 120, missing_count: 2, minimum: 0, maximum: 100,
  });
  container = document.createElement("div"); document.body.append(container);
  root = createRoot(container);
});
afterEach(async () => { await act(async () => root.unmount()); container.remove(); });
async function render(node: ReactNode) { await act(async () => root.render(node)); }
async function click(element: HTMLElement) { await act(async () => element.click()); }
async function type(input: HTMLInputElement, value: string) {
  await act(async () => {
    input.focus();
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}
async function key(input: HTMLElement, value: string, init: KeyboardEventInit = {}) {
  await act(async () => input.dispatchEvent(new KeyboardEvent("keydown", { key: value, bubbles: true, ...init })));
}
async function selectValue(select: HTMLSelectElement, value: string) {
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value")!.set!.call(select, value);
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
}
function button(label: string) {
  return container.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;
}

describe("advanced filter workspace", () => {
  function textButton(label: string) {
    return Array.from(container.querySelectorAll<HTMLButtonElement>("button"))
      .find((item) => item.textContent?.trim() === label)!;
  }
  it("uses a two-column drawer with five focused categories and no empty staged box", async () => {
    await render(<AdvancedFiltersDialog initialConditions={[]} sessionId={1} onApply={vi.fn()} onClose={vi.fn()} />);
    const dialog = container.querySelector("dialog")!;
    const body = dialog.querySelector(".advanced-filters-body")!;
    expect(dialog.classList.contains("advanced-filters-drawer")).toBe(true);
    expect(body.children).toHaveLength(2);
    expect(dialog.querySelector(".advanced-filters-summary")).toBeNull();
    expect(dialog.querySelector(".advanced-filters-selection .active-filter-list")).toBeNull();
    expect(dialog.querySelectorAll(".advanced-filters-categories button")).toHaveLength(5);
    expect(textButton("Quick filters")).toBeUndefined();
    expect(dialog.querySelector(".quick-presets")).toBeNull();
    expect(Array.from(dialog.querySelectorAll(".filter-picker-category"))
      .some((item) => item.textContent === "Quick filters")).toBe(false);
  });
  it("shows colors only in Appearance and swaps category content completely", async () => {
    await render(<AdvancedFiltersDialog initialConditions={[]} sessionId={1} onApply={vi.fn()} onClose={vi.fn()} />);
    expect(button("Yellow")).toBeTruthy();
    await click(textButton("Rating & review"));
    expect(container.querySelector(".color-filter")).toBeNull();
    expect(container.querySelector(".rating-filter")).not.toBeNull();
    await click(textButton("Image properties"));
    expect(container.querySelector(".color-filter")).toBeNull();
    expect(container.querySelector(".advanced-category-content")?.textContent).toContain("Sharpness");
    expect(container.querySelector(".filter-category-fields")?.textContent).not.toContain("Sharpness");
    await click(textButton("Camera & date"));
    expect(container.querySelector(".color-filter")).toBeNull();
    expect(container.querySelector(".rating-filter")).toBeNull();
    expect(container.querySelector(".filter-category-fields")?.textContent).not.toContain("Brightness");
    expect(container.querySelector(".advanced-category-content")?.textContent).toContain("ISO");
    expect(container.querySelector(".filter-category-fields")?.textContent).not.toContain("ISO");
    await click(textButton("Appearance"));
    expect(button("Yellow")).toBeTruthy();
  });
  it("closes an unfinished editor on category switch but keeps added filters staged", async () => {
    await render(<AdvancedFiltersDialog initialConditions={[]} sessionId={1} onApply={vi.fn()} onClose={vi.fn()} />);
    await click(Array.from(container.querySelectorAll<HTMLButtonElement>(".filter-category-field"))
      .find((item) => item.textContent?.includes("Monochrome"))!);
    expect(container.querySelector(".more-filters-panel")?.textContent).toContain("Monochrome");
    await click(textButton("Camera & date"));
    expect(container.querySelector(".more-filters-panel")).toBeNull();
    await click(textButton("Appearance"));
    expect(container.querySelector(".more-filters-panel")).toBeNull();
    await click(Array.from(container.querySelectorAll<HTMLButtonElement>(".filter-category-field"))
      .find((item) => item.textContent?.includes("Monochrome"))!);
    await click(textButton("Add filter"));
    expect(container.querySelector('[aria-label="Staged filters"]')?.textContent).toContain("Black & white");
    await click(textButton("Camera & date"));
    expect(container.querySelector(".more-filters-panel")).toBeNull();
    expect(container.querySelector('[aria-label="Staged filters"]')?.textContent).toContain("Black & white");
  });
  it("previews the exact draft match count after a short debounce", async () => {
    const loadPreviewCount = vi.fn(async (conditions: FilterCondition[]) => conditions.length ? 12 : 277);
    await render(<AdvancedFiltersDialog initialConditions={[]} sessionId={1}
      loadPreviewCount={loadPreviewCount} onApply={vi.fn()} onClose={vi.fn()} />);
    expect(container.querySelector(".advanced-filter-preview")?.textContent).toBe("Checking matches…");
    await act(async () => { await new Promise((resolve) => setTimeout(resolve, 200)); });
    expect(loadPreviewCount).toHaveBeenLastCalledWith([]);
    expect(container.querySelector(".advanced-filter-preview")?.textContent).toBe("277 matching photos");

    await click(button("Blue"));
    expect(container.querySelector(".advanced-filter-preview")?.textContent).toBe("Checking matches…");
    await act(async () => { await new Promise((resolve) => setTimeout(resolve, 200)); });
    expect(loadPreviewCount).toHaveBeenLastCalledWith([
      { field: "palette_color", operator: "in", value: ["blue"] },
    ]);
    expect(container.querySelector(".advanced-filter-preview")?.textContent).toBe("12 matching photos");
  });
  it("stages colors and only publishes the combined conditions on Apply", async () => {
    const initial: FilterCondition[] = [{ field: "rating", operator: ">=", value: 4 }];
    const apply = vi.fn(); const close = vi.fn();
    await render(<AdvancedFiltersDialog initialConditions={initial} sessionId={1} onApply={apply} onClose={close} />);
    expect(container.querySelector("dialog")?.open).toBe(true);
    await click(button("Yellow"));
    expect(apply).not.toHaveBeenCalled();
    expect(initial).toEqual([{ field: "rating", operator: ">=", value: 4 }]);
    await click(textButton("Camera & date"));
    expect(container.querySelector('[aria-label="Staged filters"]')?.textContent).toContain("Yellow");
    await click(textButton("Apply filters (2)"));
    expect(apply).toHaveBeenCalledWith([
      ...initial, { field: "palette_color", operator: "in", value: ["yellow"] },
    ]);
    expect(close).toHaveBeenCalledOnce();
  });
  it("discards draft edits on Cancel and Escape", async () => {
    const apply = vi.fn(); const close = vi.fn();
    await render(<AdvancedFiltersDialog initialConditions={[]} sessionId={1} onApply={apply} onClose={close} />);
    await click(button("Blue")); await click(textButton("Cancel"));
    expect(apply).not.toHaveBeenCalled();
    expect(close).toHaveBeenCalledOnce();
    await act(async () => container.querySelector("dialog")!.dispatchEvent(new Event("cancel", { cancelable: true })));
    expect(close).toHaveBeenCalledTimes(2);
    expect(apply).not.toHaveBeenCalled();
  });
  it("stages Clear all and applies an empty filter only on confirmation", async () => {
    const apply = vi.fn();
    await render(<AdvancedFiltersDialog initialConditions={[{ field: "iso", operator: ">=", value: 100 }]}
      sessionId={1} onApply={apply} onClose={vi.fn()} />);
    await click(textButton("Clear all"));
    expect(apply).not.toHaveBeenCalled();
    expect(container.querySelector('[aria-label="Staged filters"]')).toBeNull();
    await click(textButton("Apply filters"));
    expect(apply).toHaveBeenCalledWith([]);
  });
  it("adds a searched condition, resets the editor, and edits it without creating a duplicate", async () => {
    const apply = vi.fn();
    await render(<AdvancedFiltersDialog initialConditions={[]} sessionId={1} onApply={apply} onClose={vi.fn()} />);
    const search = container.querySelector<HTMLInputElement>('[aria-label="Search filters"]')!;

    await type(search, "iso");
    await click(container.querySelector<HTMLElement>('[role="option"]')!);
    expect(textButton("Add filter").disabled).toBe(true);
    await type(container.querySelector<HTMLInputElement>('[aria-label="ISO value"]')!, "400");
    await click(textButton("Add filter"));

    expect(container.querySelector('[aria-label="Staged filters"]')?.textContent).toContain("iso = 400");
    expect(container.querySelector(".more-filters")?.classList.contains("is-open")).toBe(false);

    await type(container.querySelector<HTMLInputElement>('[aria-label="Search filters"]')!, "iso");
    expect(container.querySelector('[role="option"]')?.textContent).toContain("Added");
    await click(container.querySelector<HTMLElement>('[role="option"]')!);
    expect(textButton("Save change")).toBeTruthy();
    const isoValue = container.querySelector<HTMLInputElement>('[aria-label="ISO value"]')!;
    expect(isoValue.value).toBe("400");
    await type(isoValue, "800");
    await click(textButton("Save change"));
    await click(textButton("Apply filters (1)"));

    expect(apply).toHaveBeenCalledWith([{ field: "iso", operator: "=", value: 800 }]);
  });
  it("adds and applies Monochrome from Search with a natural label", async () => {
    const apply = vi.fn();
    await render(<AdvancedFiltersDialog initialConditions={[]} sessionId={1} onApply={apply} onClose={vi.fn()} />);
    const search = container.querySelector<HTMLInputElement>('[aria-label="Search filters"]')!;

    await type(search, "mono");
    await click(container.querySelector<HTMLElement>('[role="option"]')!);
    expect(container.querySelector(".more-filters-panel")?.textContent).toContain("Monochrome");
    await click(textButton("Add filter"));
    expect(container.querySelector('[aria-label="Staged filters"]')?.textContent).toContain("Black & white");
    await click(textButton("Apply filters (1)"));

    expect(apply).toHaveBeenCalledWith([{ field: "monochrome", operator: "=", value: true }]);
  });
  it("keeps the applied condition unchanged when an edited draft is cancelled", async () => {
    const initial: FilterCondition[] = [{ field: "iso", operator: ">=", value: 100 }];
    const apply = vi.fn();
    await render(<AdvancedFiltersDialog initialConditions={initial} sessionId={1} onApply={apply} onClose={vi.fn()} />);

    await click(container.querySelector<HTMLButtonElement>('button[aria-label^="Edit iso"]')!);
    const isoValue = container.querySelector<HTMLInputElement>('[aria-label="ISO value"]')!;
    await type(isoValue, "200");
    await click(textButton("Save change"));
    expect(container.querySelector('[aria-label="Staged filters"]')?.textContent).toContain("iso ≥ 200");
    await click(textButton("Cancel"));

    expect(initial).toEqual([{ field: "iso", operator: ">=", value: 100 }]);
    expect(apply).not.toHaveBeenCalled();
  });
  it("opens a staged chip for in-place editing and removes it with its separate action", async () => {
    const apply = vi.fn();
    await render(<AdvancedFiltersDialog initialConditions={[{ field: "iso", operator: ">=", value: 100 }]}
      sessionId={1} onApply={apply} onClose={vi.fn()} />);

    await click(container.querySelector<HTMLButtonElement>('button[aria-label^="Edit iso"]')!);
    expect(textButton("Save change")).toBeTruthy();
    const isoValue = container.querySelector<HTMLInputElement>('[aria-label="ISO value"]')!;
    expect(isoValue.value).toBe("100");
    await type(isoValue, "200");
    await click(textButton("Save change"));
    expect(container.querySelector('[aria-label="Staged filters"]')?.textContent).toContain("iso ≥ 200");

    await click(container.querySelector<HTMLButtonElement>('button[aria-label^="Remove iso"]')!);
    expect(container.querySelector('[aria-label="Staged filters"]')).toBeNull();
    await click(textButton("Apply filters"));
    expect(apply).toHaveBeenCalledWith([]);
  });
  it("focuses discovery first and applies the complete draft with Ctrl+Enter", async () => {
    const apply = vi.fn(); const close = vi.fn();
    const initial: FilterCondition[] = [{ field: "rating", operator: ">=", value: 4 }];
    await render(<AdvancedFiltersDialog initialConditions={initial} sessionId={1} onApply={apply} onClose={close} />);
    expect(document.activeElement).toBe(container.querySelector('[aria-label="Search filters"]'));
    await key(container.querySelector("dialog")!, "Enter", { ctrlKey: true });
    expect(apply).toHaveBeenCalledWith(initial);
    expect(close).toHaveBeenCalledOnce();
  });
  it("does not reopen Search or steal focus when the category changes", async () => {
    await render(<AdvancedFiltersDialog initialConditions={[]} sessionId={1} onApply={vi.fn()} onClose={vi.fn()} />);
    const cameraCategory = textButton("Camera & date");
    await act(async () => cameraCategory.focus());
    await click(cameraCategory);
    expect(document.activeElement).toBe(cameraCategory);
    expect(container.querySelector('[aria-label="Search filters"]')?.getAttribute("aria-expanded")).toBe("false");
    expect(container.querySelector('[role="listbox"]')).toBeNull();
  });
  it("stages two measured ranges, removes one, and applies the remaining draft", async () => {
    const apply = vi.fn();
    await render(<AdvancedFiltersDialog initialConditions={[]} sessionId={1} onApply={apply} onClose={vi.fn()} />);
    await click(textButton("Image properties"));

    const rangeHeading = (label: string) => Array.from(
      container.querySelectorAll<HTMLButtonElement>(".range-filter-heading"),
    ).find((item) => item.textContent?.includes(label))!;

    await click(rangeHeading("Sharpness"));
    expect(container.querySelector('[aria-label="Sharpness range"]')?.textContent)
      .toContain("120 measured · 2 unmeasured");
    const sharpness = container.querySelector<HTMLInputElement>('[aria-label="Sharpness minimum value"]')!;
    await type(sharpness, "70");
    await key(sharpness, "Enter");

    await click(rangeHeading("Brightness"));
    const brightness = container.querySelector<HTMLInputElement>('[aria-label="Brightness minimum value"]')!;
    await type(brightness, "20");
    await key(brightness, "Enter");

    const selected = container.querySelector('[aria-label="Staged filters"]')!;
    expect(selected.textContent).toContain("sharpness ≥ 70");
    expect(selected.textContent).toContain("brightness ≥ 20");
    await click(container.querySelector<HTMLButtonElement>('button[aria-label^="Remove sharpness"]')!);
    expect(selected.textContent).not.toContain("sharpness");
    expect(selected.textContent).toContain("brightness ≥ 20");

    await click(textButton("Apply filters (1)"));
    expect(apply).toHaveBeenCalledWith([{ field: "brightness", operator: ">=", value: 20 }]);
  });
  it("closes Search before cancelling the typed editor with Escape", async () => {
    const close = vi.fn();
    await render(<AdvancedFiltersDialog initialConditions={[{ field: "iso", operator: ">=", value: 100 }]}
      sessionId={1} onApply={vi.fn()} onClose={close} />);
    await click(container.querySelector<HTMLButtonElement>('button[aria-label^="Edit iso"]')!);
    expect(textButton("Save change")).toBeTruthy();

    const search = container.querySelector<HTMLInputElement>('[aria-label="Search filters"]')!;
    await type(search, "iso");
    expect(container.querySelector('[role="listbox"]')).not.toBeNull();
    expect(textButton("Save change")).toBeTruthy();
    await key(search, "Escape");
    expect(container.querySelector('[role="listbox"]')).toBeNull();
    expect(textButton("Save change")).toBeTruthy();

    await key(container.querySelector<HTMLElement>(".more-filters-panel")!, "Escape");
    expect(Array.from(container.querySelectorAll("button")).some((item) => item.textContent === "Save change")).toBe(false);
    expect(container.querySelector("dialog")?.open).toBe(true);
    expect(close).not.toHaveBeenCalled();
  });
  it("edits legacy negative booleans as an equivalent named state", async () => {
    const apply = vi.fn();
    await render(<AdvancedFiltersDialog
      initialConditions={[{ field: "monochrome", operator: "!=", value: false }]}
      sessionId={1} onApply={apply} onClose={vi.fn()} />);
    await click(container.querySelector<HTMLButtonElement>('button[aria-label^="Edit "]')!);
    expect(container.querySelector('[aria-label="Filter condition"]')).toBeNull();
    const value = container.querySelector<HTMLSelectElement>('[aria-label="Monochrome value"]')!;
    expect(Array.from(value.options).map((option) => option.text)).toEqual(["Black & white", "Color"]);
    expect(value.value).toBe("true");
    await click(textButton("Save change"));
    await click(textButton("Apply filters (1)"));
    expect(apply).toHaveBeenCalledWith([{ field: "monochrome", operator: "=", value: true }]);
  });
  it("offers only supported natural rating comparisons and named star values", async () => {
    const apply = vi.fn();
    await render(<AdvancedFiltersDialog initialConditions={[]} sessionId={1} onApply={apply} onClose={vi.fn()} />);
    const search = container.querySelector<HTMLInputElement>('[aria-label="Search filters"]')!;
    await type(search, "rating");
    await click(container.querySelector<HTMLElement>('[role="option"]')!);
    const condition = container.querySelector<HTMLSelectElement>('[aria-label="Filter condition"]')!;
    const value = container.querySelector<HTMLSelectElement>('[aria-label="Rating value"]')!;
    expect(Array.from(condition.options).map((option) => option.text)).toEqual(["At least", "Exactly", "At most"]);
    expect(Array.from(value.options).map((option) => option.text)).toEqual([
      "Unrated", "1 star", "2 stars", "3 stars", "4 stars", "5 stars",
    ]);
    expect(value.value).toBe("1");
    await selectValue(condition, "<=");
    await selectValue(value, "3");
    await click(textButton("Add filter"));
    await click(textButton("Apply filters (1)"));
    expect(apply).toHaveBeenCalledWith([{ field: "rating", operator: "<=", value: 3 }]);
  });
});

describe("filter control interactions", () => {
  it("places filter search before the active-filter summary in the simple inspector", async () => {
    await render(<FilterBar mode="inspector" draft={[{ field: "rating", operator: ">=", value: 4 }]}
      onChange={vi.fn()} sessionId={1} />);
    const discovery = container.querySelector(".filter-discovery")!;
    expect(discovery.children[0].classList.contains("filter-picker")).toBe(true);
    expect(discovery.children[1].classList.contains("active-filter-list")).toBe(true);
  });
  it("keeps the generic editor after the ordinary inspector controls", async () => {
    await render(<FilterBar mode="inspector" draft={[{ field: "rating", operator: ">=", value: 4 }]}
      onChange={vi.fn()} sessionId={1} />);
    const input = container.querySelector<HTMLInputElement>("[role=combobox]")!;
    await type(input, "aperture");
    await click(container.querySelector<HTMLElement>("[role=option]")!);
    const panel = container.querySelector(".filterbar-panel")!;
    const classes = Array.from(panel.children).map((child) => child.className);
    expect(classes).toEqual([
      "filter-discovery",
      "color-filter",
      "quick-presets",
      "rating-filter",
      "quick-filters",
      "more-filters is-open",
    ]);
    expect(container.querySelector<HTMLSelectElement>('[aria-label="Filter condition"]')?.options[3].text)
      .toBe("at least");
    expect(container.querySelector(".filterbar-compose-add")?.classList.contains("btn-primary")).toBe(true);
  });
  it("shows persistent non-color state when a quick filter is toggled", async () => {
    function Harness() {
      const [draft, setDraft] = useState<FilterCondition[]>([]);
      return <FilterBar mode="inspector" draft={draft} onChange={setDraft} sessionId={1} />;
    }
    await render(<Harness />);
    const quick = Array.from(container.querySelectorAll<HTMLButtonElement>(".quick-presets-list button"))
      .find((item) => item.textContent?.includes("Black & white"))!;
    await click(quick);
    expect(quick.getAttribute("aria-pressed")).toBe("true");
    expect(quick.querySelector(".quick-preset-check")?.textContent).toBe("✓");
    await click(quick);
    expect(quick.getAttribute("aria-pressed")).toBe("false");
    expect(quick.querySelector(".quick-preset-check")?.textContent).toBe("");
  });
  it("adds the visible default value for a boolean filter", async () => {
    const changed = vi.fn();
    await render(<FilterBar mode="inspector" draft={[]} onChange={changed} sessionId={1} />);
    const input = container.querySelector<HTMLInputElement>("[role=combobox]")!;
    await type(input, "monochrome");
    await click(Array.from(container.querySelectorAll<HTMLElement>("[role=option]"))
      .find((item) => item.textContent?.trim() === "Monochrome")!);
    const value = container.querySelector<HTMLSelectElement>('[aria-label="Monochrome value"]')!;
    expect(container.querySelector('[aria-label="Filter condition"]')).toBeNull();
    expect(Array.from(value.options).map((option) => option.text)).toEqual(["Black & white", "Color"]);
    expect(value.value).toBe("true");
    await click(Array.from(container.querySelectorAll<HTMLButtonElement>("button"))
      .find((item) => item.textContent?.trim() === "Add filter")!);
    expect(changed).toHaveBeenCalledWith([{ field: "monochrome", operator: "=", value: true }]);
  });
  it("searches, selects with the keyboard and closes with Escape", async () => {
    const select = vi.fn();
    await render(<FilterPicker draft={[{ field: "iso", operator: ">=", value: 100 }]} onSelect={select} />);
    const input = container.querySelector<HTMLInputElement>("[role=combobox]")!;
    await type(input, "iso");
    expect(container.querySelectorAll("[role=option]")).toHaveLength(1);
    expect(container.querySelector("[role=option]")?.textContent).toContain("Added");
    await key(input, "ArrowDown"); await key(input, "Enter");
    expect(select.mock.calls[0][0].field).toBe("iso");
    expect(input.getAttribute("aria-expanded")).toBe("false");
    await type(input, "face");
    expect(container.querySelectorAll("[role=option]").length).toBeGreaterThan(1);
    await key(input, "Escape");
    expect(container.querySelector("[role=listbox]")).toBeNull();
    expect(document.activeElement).toBe(input);
  });
  it("removes individual color chips without losing other conditions, then clears all", async () => {
    function Harness() {
      const [draft, setDraft] = useState<FilterCondition[]>([
        { field: "palette_color", operator: "in", value: ["yellow", "blue"] },
        { field: "iso", operator: ">=", value: 100 },
      ]);
      return <><ActiveFilterList draft={draft} onChange={setDraft} />
        <button onClick={() => setDraft([])}>Clear all</button><output>{JSON.stringify(draft)}</output></>;
    }
    await render(<Harness />);
    await click(button("Remove Yellow"));
    expect(container.querySelector("output")?.textContent).toContain('"value":["blue"]');
    expect(container.querySelector("output")?.textContent).toContain('"field":"iso"');
    await click(button("Remove Blue"));
    expect(container.querySelector("output")?.textContent).not.toContain("palette_color");
    await click(Array.from(container.querySelectorAll("button")).find((item) => item.textContent === "Clear all")!);
    expect(container.querySelector("[aria-label='Active filters']")).toBeNull();
  });
  it("supports multi-select colors with non-color selection feedback", async () => {
    function Harness() {
      const [draft, setDraft] = useState<FilterCondition[]>([]);
      return <ColorSpectrumFilter draft={draft} onChange={setDraft} />;
    }
    await render(<Harness />);
    await click(button("Yellow")); await click(button("Blue"));
    expect(button("Yellow").getAttribute("aria-pressed")).toBe("true");
    expect(button("Yellow").textContent).toContain("✓");
    expect(button("Blue").getAttribute("aria-pressed")).toBe("true");
    await click(button("Yellow"));
    expect(button("Yellow").getAttribute("aria-pressed")).toBe("false");
    expect(button("Blue").getAttribute("aria-pressed")).toBe("true");
  });
  it("expands measured ranges independently and preserves them across a numeric commit", async () => {
    function Harness() {
      const [draft, setDraft] = useState<FilterCondition[]>([{ field: "rating", operator: ">=", value: 4 }]);
      return <><QuickFilterControls draft={draft} onChange={setDraft} sessionId={1} />
        <output>{JSON.stringify(draft)}</output></>;
    }
    await render(<Harness />);
    const headings = container.querySelectorAll<HTMLButtonElement>(".range-filter-heading");
    expect(Array.from(headings).every((heading) => heading.getAttribute("aria-expanded") === "false")).toBe(true);
    await click(headings[0]); await click(headings[1]); await click(headings[2]);
    expect(headings[0].getAttribute("aria-expanded")).toBe("true");
    expect(headings[1].getAttribute("aria-expanded")).toBe("true");
    expect(headings[2].getAttribute("aria-expanded")).toBe("true");
    const input = container.querySelector<HTMLInputElement>('[aria-label="Sharpness minimum value"]')!;
    expect(input.step).toBe("1");
    await type(input, "70"); await key(input, "Enter");
    expect(container.querySelector("output")?.textContent).toContain('"field":"sharpness","operator":">=","value":70');
    expect(container.querySelector("output")?.textContent).toContain('"field":"rating"');
    expect(headings[0].getAttribute("aria-expanded")).toBe("true");
    expect(headings[1].getAttribute("aria-expanded")).toBe("true");
    expect(headings[2].getAttribute("aria-expanded")).toBe("true");
    await click(headings[1]);
    expect(headings[0].getAttribute("aria-expanded")).toBe("true");
    expect(headings[1].getAttribute("aria-expanded")).toBe("false");
    expect(headings[2].getAttribute("aria-expanded")).toBe("true");
    expect(container.querySelector<HTMLFieldSetElement>('[aria-label="Brightness range"]')?.disabled).toBe(false);
    expect(container.querySelector<HTMLFieldSetElement>('[aria-label="Sharpness range"]')?.disabled).toBe(true);
    expect(container.querySelector<HTMLFieldSetElement>('[aria-label="Contrast range"]')?.disabled).toBe(false);
  });
  it("omits meaningless numeric controls when a field has no recorded values", async () => {
    vi.mocked(api.numericFilterStats).mockImplementation(async (field) => field === "brightness"
      ? { recorded_count: 0, missing_count: 120, minimum: null, maximum: null }
      : { recorded_count: 120, missing_count: 0, minimum: 0, maximum: 100 });
    const changed = vi.fn();
    await render(<QuickFilterControls draft={[]} onChange={changed} sessionId={1} />);
    await click(container.querySelectorAll<HTMLButtonElement>(".range-filter-heading")[0]);
    const section = container.querySelector('[aria-label="Brightness range"]')!;
    expect(section.textContent).toContain("Not recorded in this shoot");
    expect(section.querySelector('[aria-label="Brightness minimum"]')).toBeNull();
    expect(section.querySelector('[aria-label="Brightness minimum value"]')).toBeNull();
    const missingOnly = Array.from(section.querySelectorAll<HTMLButtonElement>("button"))
      .find((item) => item.textContent?.includes("Unmeasured only"))!;
    expect(missingOnly.disabled).toBe(false);
    await click(missingOnly);
    expect(changed).toHaveBeenCalledWith([{ field: "brightness", operator: "is-null", value: null }]);
  });
  it("preserves strict custom conditions and exposes unmeasured-only semantics", async () => {
    const changed = vi.fn();
    await render(<QuickFilterControls draft={[{ field: "sharpness", operator: "<", value: 30 }]}
      onChange={changed} sessionId={1} />);
    await click(container.querySelectorAll<HTMLButtonElement>(".range-filter-heading")[1]);
    expect(container.querySelector<HTMLInputElement>('[aria-label="Sharpness minimum"]')?.disabled).toBe(true);
    expect(changed).not.toHaveBeenCalled();
    const section = container.querySelector('[aria-label="Sharpness range"]')!;
    await click(Array.from(section.querySelectorAll("button")).find((item) => item.textContent?.includes("Unmeasured only"))!);
    expect(changed).toHaveBeenCalledWith([{ field: "sharpness", operator: "is-null", value: null }]);
  });
  it("does not normalize a recorded-only condition when tabbing through unchanged controls", async () => {
    const changed = vi.fn();
    await render(<QuickFilterControls draft={[{ field: "sharpness", operator: "not-null", value: null }]}
      onChange={changed} sessionId={1} />);
    await click(container.querySelectorAll<HTMLButtonElement>(".range-filter-heading")[1]);
    for (const label of ["Sharpness minimum", "Sharpness minimum value"]) {
      const input = container.querySelector<HTMLInputElement>(`[aria-label="${label}"]`)!;
      await act(async () => { input.focus(); input.blur(); });
    }
    expect(changed).not.toHaveBeenCalled();
  });
});
