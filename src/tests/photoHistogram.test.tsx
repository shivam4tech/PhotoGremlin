// @vitest-environment happy-dom
import { act, useState, type ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { PhotoHistogram, histogramAreaPath, type HistogramMode } from "@/components/PhotoHistogram";
import type { PhotoHistogram as PhotoHistogramData } from "@/types/api";

Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });

function bins(entries: Array<[number, number]>): number[] {
  const values = Array<number>(256).fill(0);
  for (const [index, count] of entries) values[index] = count;
  return values;
}

const histogram: PhotoHistogramData = {
  source: "rendered_preview",
  pixel_count: 10,
  luma: bins([[0, 2], [128, 10], [255, 4]]),
  red: bins([[32, 5]]),
  green: bins([[128, 10]]),
  blue: bins([[224, 2]]),
};

let root: Root;
let container: HTMLDivElement;

beforeEach(() => {
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
});

async function render(node: ReactNode) {
  await act(async () => root.render(node));
}

async function click(element: HTMLElement) {
  await act(async () => element.click());
}

function modeButton(label: string): HTMLButtonElement {
  return Array.from(container.querySelectorAll<HTMLButtonElement>("button"))
    .find((button) => button.textContent === label)!;
}

function HistogramHarness({ data }: { data: PhotoHistogramData }) {
  const [mode, setMode] = useState<HistogramMode>("luma");
  return <PhotoHistogram histogram={data} mode={mode} onModeChange={setMode} />;
}

describe("PhotoHistogram", () => {
  it("defaults to factual luma and switches to shared-scale RGB paths", async () => {
    await render(<HistogramHarness data={histogram} />);

    expect(modeButton("Luma").getAttribute("aria-pressed")).toBe("true");
    expect(container.querySelector("svg")?.getAttribute("aria-label")).toContain("Luminance");
    expect(container.querySelectorAll("svg path")).toHaveLength(1);
    expect(container.textContent).toContain("Rendered preview");

    await click(modeButton("RGB"));

    expect(modeButton("RGB").getAttribute("aria-pressed")).toBe("true");
    expect(container.querySelector("svg")?.getAttribute("aria-label")).toContain("Red, green, and blue");
    expect(container.querySelectorAll("svg path")).toHaveLength(3);
    expect(container.querySelector(".photo-histogram-red")?.getAttribute("d")).toContain("32.13 40.00");
    expect(container.querySelector(".photo-histogram-green")?.getAttribute("d")).toContain("128.50 0.00");
  });

  it("keeps the selected channel mode when the focused preview changes", async () => {
    await render(<HistogramHarness data={histogram} />);
    await click(modeButton("RGB"));

    await render(<HistogramHarness data={{ ...histogram, pixel_count: 12 }} />);

    expect(modeButton("RGB").getAttribute("aria-pressed")).toBe("true");
    expect(container.querySelectorAll("svg path")).toHaveLength(3);
  });

  it("produces a finite baseline for an empty histogram", () => {
    const path = histogramAreaPath([], 0);
    expect(path).not.toMatch(/NaN|Infinity/);
    expect(path.startsWith("M 0 80 L 0.00 80.00")).toBe(true);
    expect(path.endsWith("L 256 80 Z")).toBe(true);
  });
});
