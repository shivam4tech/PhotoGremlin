import { useLayoutEffect, useRef, useState } from "react";

/** Gap between tiles, in pixels. Must match the CSS on `.vg-row`. */
export const TILE_GAP = 8;
/** Grid inner padding, in pixels. Must match the CSS on `.vg-inner`. */
export const GRID_PAD = 12;

export interface VirtualGridLayout {
  cols: number;
  rows: number;
  totalHeight: number;
  /** First item index to render (inclusive). */
  startIndex: number;
  /** Last item index to render (exclusive). */
  endIndex: number;
}

/**
 * Pure windowing math (unit-tested): given scroll state and geometry, which
 * items actually need to be mounted. Mounting only ~1–3 screens of rows is
 * what keeps a 50k-photo library smooth — the browser never holds DOM or
 * decoded images for off-screen tiles.
 */
export function computeVisibleRange(
  itemCount: number,
  cols: number,
  viewportH: number,
  scrollTop: number,
  rowH: number,
  overscanRows = 2,
): VirtualGridLayout {
  const safeCols = Math.max(1, Math.floor(cols) || 1);
  const rows = Math.ceil(itemCount / safeCols);
  // Clamp to the last row so a scroll position beyond the content (or a
  // very short library) still yields a start <= end window pinned at the end.
  const firstRow = Math.min(
    Math.max(0, Math.floor(scrollTop / rowH) - overscanRows),
    Math.max(0, rows - 1),
  );
  const visibleRows = Math.ceil(viewportH / rowH) + overscanRows * 2;
  const lastRow = Math.min(rows, firstRow + Math.max(0, visibleRows));
  return {
    cols: safeCols,
    rows,
    totalHeight: rows * rowH,
    startIndex: firstRow * safeCols,
    endIndex: Math.min(itemCount, lastRow * safeCols),
  };
}

/** How many columns fit a container of the given width. */
export function computeColumns(containerW: number, minColW: number): number {
  if (containerW <= 0) return 1;
  return Math.max(1, Math.floor((containerW + TILE_GAP) / (minColW + TILE_GAP)));
}

export interface VirtualGridProps {
  itemCount: number;
  minColWidth?: number;
  /** Fixed square-ish row height (tile + gap). */
  rowHeight?: number;
  overscanRows?: number;
  /** Mount one item per visible index. Kept stable across scrolls. */
  render: (index: number) => React.ReactNode;
}

/**
 * Dependency-free vertical virtualizer: a scroll container holding only the
 * rows in view (plus overscan), absolutely positioned inside a full-height
 * spacer. Rows are `display:grid`, so column count can change live on
 * resize without re-measuring per item.
 */
export function VirtualGrid({
  itemCount,
  minColWidth = 168,
  rowHeight = 180,
  overscanRows = 2,
  render,
}: VirtualGridProps) {
  const ref = useRef<HTMLDivElement>(null);
  const [box, setBox] = useState({ w: 0, h: 0 });
  const [scrollTop, setScrollTop] = useState(0);
  const raf = useRef(0);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const measure = () => setBox({ w: el.clientWidth, h: el.clientHeight });
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => {
      ro.disconnect();
      cancelAnimationFrame(raf.current);
    };
  }, []);

  function onScroll() {
    const el = ref.current;
    if (!el) return;
    if (raf.current) return; // throttle to one state update per frame
    raf.current = requestAnimationFrame(() => {
      raf.current = 0;
      setScrollTop(el.scrollTop);
    });
  }

  const cols = computeColumns(box.w, minColWidth);
  const layout = computeVisibleRange(itemCount, cols, box.h, scrollTop, rowHeight, overscanRows);

  const rowsRender: React.ReactNode[] = [];
  for (let row = Math.floor(layout.startIndex / layout.cols); row < Math.ceil(layout.endIndex / layout.cols); row++) {
    const rowStart = row * layout.cols;
    if (rowStart >= itemCount) break;
    rowsRender.push(
      <div
        key={row}
        className="vg-row"
        style={{
          position: "absolute",
          top: row * rowHeight,
          left: 0,
          right: 0,
          height: rowHeight - TILE_GAP,
          display: "grid",
          gridTemplateColumns: `repeat(${layout.cols}, 1fr)`,
          gap: TILE_GAP,
          padding: `0 ${GRID_PAD}px ${TILE_GAP}px`,
        }}
      >
        {Array.from({ length: Math.min(layout.cols, itemCount - rowStart) }, (_, c) => {
          const i = rowStart + c;
          return <div key={i} className="vg-cell">{render(i)}</div>;
        })}
      </div>,
    );
  }

  return (
    <div ref={ref} className="vg" onScroll={onScroll}>
      <div className="vg-inner" style={{ height: layout.totalHeight, position: "relative" }}>
        {rowsRender}
      </div>
    </div>
  );
}
