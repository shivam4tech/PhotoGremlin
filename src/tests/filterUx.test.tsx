// @vitest-environment happy-dom
import { act, useState, type ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { FilterPicker } from "@/features/library/FilterPicker";
import { ActiveFilterList } from "@/features/library/ActiveFilterList";
import { QuickFilterControls } from "@/features/library/QuickFilterControls";
import { ColorSpectrumFilter } from "@/features/library/ColorSpectrumFilter";
import { AdvancedFiltersDialog } from "@/features/library/AdvancedFiltersDialog";
import type { FilterCondition } from "@/types/api";

vi.mock("@/lib/ipc", () => ({ api: {
  numericFilterStats: vi.fn(async () => ({ recorded_count: 120, missing_count: 2 })),
} }));
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
let root: Root;
let container: HTMLDivElement;
beforeEach(() => {
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
async function key(input: HTMLElement, value: string) {
  await act(async () => input.dispatchEvent(new KeyboardEvent("keydown", { key: value, bubbles: true })));
}
function button(label: string) {
  return container.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;
}

describe("advanced filter workspace", () => {
  function textButton(label: string) {
    return Array.from(container.querySelectorAll<HTMLButtonElement>("button"))
      .find((item) => item.textContent?.trim() === label)!;
  }
  it("stages colors and only publishes the combined conditions on Apply", async () => {
    const initial: FilterCondition[] = [{ field: "rating", operator: ">=", value: 4 }];
    const apply = vi.fn(); const close = vi.fn();
    await render(<AdvancedFiltersDialog initialConditions={initial} sessionId={1} onApply={apply} onClose={close} />);
    expect(container.querySelector("dialog")?.open).toBe(true);
    await click(button("Yellow"));
    expect(apply).not.toHaveBeenCalled();
    expect(initial).toEqual([{ field: "rating", operator: ">=", value: 4 }]);
    await click(textButton("Camera & date"));
    expect(container.querySelector('[aria-label="Selected filters"]')?.textContent).toContain("Yellow");
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
    expect(container.querySelector('[aria-label="Selected filters"]')).toBeNull();
    await click(textButton("Apply filters"));
    expect(apply).toHaveBeenCalledWith([]);
  });
});

describe("filter control interactions", () => {
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
  it("expands one range, commits a numeric value and preserves the open editor", async () => {
    function Harness() {
      const [draft, setDraft] = useState<FilterCondition[]>([{ field: "rating", operator: ">=", value: 4 }]);
      return <><QuickFilterControls draft={draft} onChange={setDraft} sessionId={1} />
        <output>{JSON.stringify(draft)}</output></>;
    }
    await render(<Harness />);
    const headings = container.querySelectorAll<HTMLButtonElement>(".range-filter-heading");
    expect(Array.from(headings).every((heading) => heading.getAttribute("aria-expanded") === "false")).toBe(true);
    await click(headings[0]); await click(headings[1]);
    expect(headings[0].getAttribute("aria-expanded")).toBe("false");
    const input = container.querySelector<HTMLInputElement>('[aria-label="Sharpness minimum value"]')!;
    await type(input, "70"); await key(input, "Enter");
    expect(container.querySelector("output")?.textContent).toContain('"field":"sharpness","operator":">=","value":70');
    expect(container.querySelector("output")?.textContent).toContain('"field":"rating"');
    expect(headings[1].getAttribute("aria-expanded")).toBe("true");
    expect(container.querySelector<HTMLFieldSetElement>('[aria-label="Brightness range"]')?.disabled).toBe(true);
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
