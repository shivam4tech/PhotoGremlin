import { useEffect, useRef, useState } from "react";
import { api, toErrorMessage } from "@/lib/ipc";
import type { SelectionState } from "@/stores/appStore";
import type { PhotoSummary } from "@/types/api";

type PreviewState =
  | { kind: "loading" }
  | { kind: "ready"; url: string }
  | { kind: "unavailable"; message: string };

const ZOOM_STEPS = [1, 1.5, 2, 3, 4] as const;

function decisionLabel(state: SelectionState | null): string {
  if (state === "selected") return "Kept";
  if (state === "rejected") return "Rejected";
  if (state === "needs_attention") return "Later";
  return "Undecided";
}

export function ReviewCompareDialog({
  photos,
  focusedId,
  selections,
  onFocus,
  onClose,
}: {
  photos: PhotoSummary[];
  focusedId: number;
  selections: Record<number, SelectionState>;
  onFocus: (photoId: number) => void;
  onClose: () => void;
}) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const viewportRefs = useRef<Array<HTMLDivElement | null>>([]);
  const syncingRef = useRef(false);
  const [previews, setPreviews] = useState<Record<number, PreviewState>>(() =>
    Object.fromEntries(photos.map((photo) => [photo.id, { kind: "loading" }])),
  );
  const [zoomIndex, setZoomIndex] = useState(0);
  const zoom = ZOOM_STEPS[zoomIndex];

  useEffect(() => {
    const dialog = dialogRef.current;
    if (dialog && !dialog.open) dialog.showModal();
    return () => { if (dialog?.open) dialog.close(); };
  }, []);

  useEffect(() => {
    let cancelled = false;
    setPreviews(Object.fromEntries(photos.map((photo) => [photo.id, { kind: "loading" }])));
    for (const photo of photos) {
      api.getThumbnail(photo.id, "viewer")
        .then((thumbnail) => {
          if (!cancelled) {
            setPreviews((current) => ({
              ...current,
              [photo.id]: { kind: "ready", url: thumbnail.data_url },
            }));
          }
        })
        .catch((error) => {
          if (!cancelled) {
            setPreviews((current) => ({
              ...current,
              [photo.id]: { kind: "unavailable", message: toErrorMessage(error) },
            }));
          }
        });
    }
    return () => { cancelled = true; };
  }, [photos]);

  useEffect(() => {
    for (const viewport of viewportRefs.current) {
      if (!viewport) continue;
      viewport.scrollTo({
        left: (viewport.scrollWidth - viewport.clientWidth) / 2,
        top: (viewport.scrollHeight - viewport.clientHeight) / 2,
      });
    }
  }, [zoom]);

  function synchronizePan(sourceIndex: number) {
    if (syncingRef.current) return;
    const source = viewportRefs.current[sourceIndex];
    if (!source) return;
    const xRange = Math.max(1, source.scrollWidth - source.clientWidth);
    const yRange = Math.max(1, source.scrollHeight - source.clientHeight);
    const x = source.scrollLeft / xRange;
    const y = source.scrollTop / yRange;
    syncingRef.current = true;
    viewportRefs.current.forEach((viewport, index) => {
      if (!viewport || index === sourceIndex) return;
      viewport.scrollLeft = x * Math.max(0, viewport.scrollWidth - viewport.clientWidth);
      viewport.scrollTop = y * Math.max(0, viewport.scrollHeight - viewport.clientHeight);
    });
    window.requestAnimationFrame(() => { syncingRef.current = false; });
  }

  return (
    <dialog
      ref={dialogRef}
      className="compare-dialog"
      aria-labelledby="compare-dialog-title"
      onCancel={(event) => { event.preventDefault(); onClose(); }}
    >
      <div className="compare-dialog-shell">
        <header className="compare-dialog-head">
          <div>
            <strong id="compare-dialog-title">Compare sequence</strong>
            <span>Zoom and pan stay synchronized across these local previews</span>
          </div>
          <div className="compare-zoom" aria-label="Comparison zoom controls">
            <button
              className="btn btn-sm"
              disabled={zoomIndex === 0}
              onClick={() => setZoomIndex((current) => Math.max(0, current - 1))}
              aria-label="Zoom out"
            >−</button>
            <output className="mono" aria-live="polite">{Math.round(zoom * 100)}%</output>
            <button
              className="btn btn-sm"
              disabled={zoomIndex === ZOOM_STEPS.length - 1}
              onClick={() => setZoomIndex((current) => Math.min(ZOOM_STEPS.length - 1, current + 1))}
              aria-label="Zoom in"
            >+</button>
          </div>
          <button className="btn btn-ghost btn-sm" onClick={onClose} aria-label="Close comparison">✕</button>
        </header>

        <div className={`compare-grid compare-grid-${photos.length}`}>
          {photos.map((photo, index) => {
            const preview = previews[photo.id] ?? { kind: "loading" };
            const state = selections[photo.id] ?? null;
            return (
              <article className={`compare-frame${photo.id === focusedId ? " is-focused" : ""}`} key={photo.id}>
                <div
                  className="compare-viewport"
                  ref={(node) => { viewportRefs.current[index] = node; }}
                  onScroll={() => synchronizePan(index)}
                >
                  {preview.kind === "ready" && (
                    <div className="compare-image-canvas" style={{ width: `${zoom * 100}%`, height: `${zoom * 100}%` }}>
                      <img src={preview.url} alt={photo.filename} draggable={false} />
                    </div>
                  )}
                  {preview.kind === "loading" && <div className="compare-placeholder">Loading local preview…</div>}
                  {preview.kind === "unavailable" && (
                    <div className="compare-placeholder">Preview unavailable<span>{preview.message}</span></div>
                  )}
                </div>
                <footer className="compare-frame-meta">
                  <div>
                    <strong className="mono" title={photo.filename}>{photo.filename}</strong>
                    <span>{decisionLabel(state)}</span>
                  </div>
                  <button
                    className={`btn btn-sm${photo.id === focusedId ? " btn-primary" : ""}`}
                    onClick={() => { onFocus(photo.id); onClose(); }}
                  >{photo.id === focusedId ? "Focused" : "Focus frame"}</button>
                </footer>
              </article>
            );
          })}
        </div>
      </div>
    </dialog>
  );
}
