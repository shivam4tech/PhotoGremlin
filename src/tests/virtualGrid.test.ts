import { describe, it, expect } from "vitest";
import { computeVisibleRange, computeColumns } from "@/components/VirtualGrid";

const ROW_H = 180;

describe("computeColumns", () => {
  it("fits as many columns as the width allows", () => {
    // width 700, minCol 168, gap 8 -> floor((700+8)/(168+8)) = floor(708/176) = 4
    expect(computeColumns(700, 168)).toBe(4);
    // a full single column width
    expect(computeColumns(168, 168)).toBe(1);
    // one pixel short stays at the same count only if it still fits
    expect(computeColumns(167, 168)).toBe(1);
  });

  it("never goes below one column", () => {
    expect(computeColumns(0, 168)).toBe(1);
    expect(computeColumns(10, 168)).toBe(1);
  });
});

describe("computeVisibleRange", () => {
  it("covers the viewport plus overscan at scroll top", () => {
    // viewport 900 tall / 180 per row = 5 rows visible, +2*2 overscan = 9 rows.
    const layout = computeVisibleRange(1000, 4, 900, 0, ROW_H, 2);
    expect(layout.startIndex).toBe(0);
    // rows: ceil(1000/4)=250 ; firstRow=0 ; lastRow=min(250, 0+5+4)=9
    expect(layout.endIndex).toBe(Math.min(1000, 9 * 4));
    expect(layout.totalHeight).toBe(250 * ROW_H);
    expect(layout.cols).toBe(4);
    // startIndex < endIndex always
    expect(layout.endIndex).toBeGreaterThan(layout.startIndex);
  });

  it("starts below the top row once scrolled", () => {
    // scrolled 1800px down => row 10 is the first visible; minus 2 overscan = 8
    const layout = computeVisibleRange(1000, 4, 900, 1800, ROW_H, 2);
    expect(layout.startIndex).toBe(8 * 4);
  });

  it("never reads past the item count", () => {
    // Near the end: only a few items remain.
    const layout = computeVisibleRange(10, 4, 900, 1000000, ROW_H, 2);
    expect(layout.endIndex).toBeLessThanOrEqual(10);
    expect(layout.startIndex).toBeLessThanOrEqual(layout.endIndex);
  });

  it("returns an empty-safe range for zero items", () => {
    const layout = computeVisibleRange(0, 4, 900, 0, ROW_H, 2);
    expect(layout.endIndex).toBe(0);
    expect(layout.startIndex).toBe(0);
    expect(layout.totalHeight).toBe(0);
  });

  it("clamps an abnormal column count to at least one", () => {
    const layout = computeVisibleRange(20, 0, 900, 0, ROW_H, 2);
    expect(layout.cols).toBe(1);
    // With 1 col, 20 items = 20 rows; overscan keeps first 9 rows.
    expect(layout.startIndex).toBe(0);
    expect(layout.endIndex).toBe(Math.min(20, 9 * 1));
  });

  it("keeps endIndex within startIndex + visible window", () => {
    // A wide viewport should not mount absurdly many items at once.
    const layout = computeVisibleRange(100_000, 6, 4000, 0, ROW_H, 2);
    const mounted = layout.endIndex - layout.startIndex;
    // visible rows = ceil(4000/180)=23 + 4 overshoot = 27 rows * 6 cols
    expect(mounted).toBeLessThanOrEqual(27 * 6);
  });
});
