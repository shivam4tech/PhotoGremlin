import { useEffect, useMemo, useState } from "react";
import { api, toErrorMessage } from "@/lib/ipc";
import { useAppStore, type SelectionState } from "@/stores/appStore";
import { buildReviewUnits, firstUnreviewedId, reviewCounts } from "@/features/review/reviewQueue";
import type { PhotoFull, PhotoSummary, ReviewQueue } from "@/types/api";

type ImageState = { kind: "loading" } | { kind: "ready"; url: string } | { kind: "unavailable"; message: string };
type LastAction = { photoId: number; previous: SelectionState | null; unitIndex: number };

function ReviewImage({ photo }: { photo: PhotoSummary }) {
  const [image, setImage] = useState<ImageState>({ kind: "loading" });
  useEffect(() => {
    let cancelled = false;
    setImage({ kind: "loading" });
    api.getThumbnail(photo.id, "viewer")
      .then((thumbnail) => { if (!cancelled) setImage({ kind: "ready", url: thumbnail.data_url }); })
      .catch((error) => { if (!cancelled) setImage({ kind: "unavailable", message: toErrorMessage(error) }); });
    return () => { cancelled = true; };
  }, [photo.id]);

  if (image.kind === "ready") return <img className="review-image" src={image.url} alt={photo.filename} />;
  if (image.kind === "unavailable") return <div className="review-image-placeholder">Preview unavailable<br /><span>{image.message}</span></div>;
  return <div className="review-image-placeholder">Loading local preview…</div>;
}

function ReviewThumb({ photo, active, state, onFocus }: {
  photo: PhotoSummary;
  active: boolean;
  state: SelectionState | null;
  onFocus: () => void;
}) {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    api.getThumbnail(photo.id, "grid").then((thumbnail) => {
      if (!cancelled) setUrl(thumbnail.data_url);
    }).catch(() => { if (!cancelled) setUrl(null); });
    return () => { cancelled = true; };
  }, [photo.id]);
  return (
    <button
      className={`review-thumb${active ? " is-active" : ""}${state ? ` is-${state}` : ""}`}
      onClick={onFocus}
      title={photo.filename}
      aria-label={`Review ${photo.filename}${state ? `, ${state.replace("_", " ")}` : ""}`}
    >
      {url ? <img src={url} alt="" /> : <span>{photo.extension.toUpperCase()}</span>}
    </button>
  );
}

function measurementLabel(full: PhotoFull | null): string | null {
  if (!full) return null;
  const parts: string[] = [];
  if (full.sharpness != null) parts.push(`sharpness ${Math.round(full.sharpness)}`);
  if (full.highlight_clipping != null && full.highlight_clipping > 0.01) parts.push(`highlights ${(full.highlight_clipping * 100).toFixed(1)}%`);
  if (full.shadow_clipping != null && full.shadow_clipping > 0.01) parts.push(`shadows ${(full.shadow_clipping * 100).toFixed(1)}%`);
  return parts.length > 0 ? parts.join(" · ") : null;
}

/** Keyboard-first, local-only decision flow for one indexed shoot. */
export function ReviewMode({ sessionId, sessionName, onClose }: { sessionId: number; sessionName: string; onClose: () => void }) {
  const selections = useAppStore((state) => state.selections);
  const store = useAppStore.getState;
  const [queue, setQueue] = useState<ReviewQueue | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [unitIndex, setUnitIndex] = useState(0);
  const [focusedId, setFocusedId] = useState<number | null>(null);
  const [full, setFull] = useState<PhotoFull | null>(null);
  const [lastAction, setLastAction] = useState<LastAction | null>(null);

  useEffect(() => {
    let cancelled = false;
    setQueue(null);
    setLoadError(null);
    setUnitIndex(0);
    setFocusedId(null);
    Promise.all([
      store().loadSelections(sessionId),
      api.reviewQueue(sessionId),
      api.getReviewProgress(sessionId),
    ]).then(([, nextQueue, progress]) => {
      if (cancelled) return;
      const nextUnits = buildReviewUnits(nextQueue.photos, nextQueue.sequences);
      const restoredIndex = Math.max(0, Math.min(nextUnits.length - 1, progress?.unit_index ?? 0));
      const restoredUnit = nextUnits[restoredIndex];
      setQueue(nextQueue);
      setUnitIndex(restoredIndex);
      setFocusedId(
        progress?.focused_photo_id != null && restoredUnit?.photoIds.includes(progress.focused_photo_id)
          ? progress.focused_photo_id
          : null,
      );
    }).catch((error) => { if (!cancelled) setLoadError(toErrorMessage(error)); });
    return () => { cancelled = true; };
  }, [sessionId]);

  const photosById = useMemo(() => new Map(queue?.photos.map((photo) => [photo.id, photo]) ?? []), [queue]);
  const units = useMemo(() => buildReviewUnits(queue?.photos ?? [], queue?.sequences ?? []), [queue]);
  const allPhotoIds = useMemo(() => queue?.photos.map((photo) => photo.id) ?? [], [queue]);
  const currentUnit = units[unitIndex] ?? null;
  const currentPhoto = focusedId === null ? null : photosById.get(focusedId) ?? null;
  const counts = reviewCounts(allPhotoIds, selections);

  useEffect(() => {
    if (!currentUnit) return;
    if (focusedId === null || !currentUnit.photoIds.includes(focusedId)) {
      setFocusedId(firstUnreviewedId(currentUnit, selections));
    }
  }, [currentUnit, focusedId, selections]);

  useEffect(() => {
    let cancelled = false;
    setFull(null);
    if (focusedId === null) return;
    api.getPhotoFull(focusedId).then((photo) => { if (!cancelled) setFull(photo); }).catch(() => {});
    return () => { cancelled = true; };
  }, [focusedId]);

  useEffect(() => {
    if (queue === null || currentUnit === null) return;
    const timer = window.setTimeout(() => {
      void api.setReviewProgress(sessionId, unitIndex, focusedId).catch(() => {
        // Progress persistence is best-effort; review decisions are stored separately.
      });
    }, 250);
    return () => window.clearTimeout(timer);
  }, [sessionId, queue, currentUnit, unitIndex, focusedId]);

  function moveToUnit(nextIndex: number) {
    const bounded = Math.max(0, Math.min(units.length - 1, nextIndex));
    const nextUnit = units[bounded];
    if (!nextUnit) return;
    setUnitIndex(bounded);
    setFocusedId(firstUnreviewedId(nextUnit, selections));
  }

  function decide(state: SelectionState) {
    if (!currentUnit || focusedId === null) return;
    const previous = selections[focusedId] ?? null;
    store().setSelection(focusedId, state);
    setLastAction({ photoId: focusedId, previous, unitIndex });

    const position = currentUnit.photoIds.indexOf(focusedId);
    const nextInUnit = [...currentUnit.photoIds.slice(position + 1), ...currentUnit.photoIds.slice(0, position)]
      .find((id) => !selections[id]);
    if (nextInUnit !== undefined) setFocusedId(nextInUnit);
    else moveToUnit(unitIndex + 1);
  }

  function clearDecision() {
    if (focusedId === null) return;
    const previous = selections[focusedId] ?? null;
    if (previous === null) return;
    store().setSelection(focusedId, null);
    setLastAction({ photoId: focusedId, previous, unitIndex });
  }

  function undo() {
    if (!lastAction) return;
    store().setSelection(lastAction.photoId, lastAction.previous);
    setUnitIndex(lastAction.unitIndex);
    setFocusedId(lastAction.photoId);
    setLastAction(null);
  }

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      const target = event.target as HTMLElement | null;
      if (target?.closest("input, textarea, select, [contenteditable='true']")) return;
      const key = event.key.toLowerCase();
      if (key === "escape") { event.preventDefault(); onClose(); }
      else if (key === "arrowleft" || key === "h") { event.preventDefault(); moveToUnit(unitIndex - 1); }
      else if (key === "arrowright" || key === "j") { event.preventDefault(); moveToUnit(unitIndex + 1); }
      else if (key === "k") { event.preventDefault(); decide("selected"); }
      else if (key === "x") { event.preventDefault(); decide("rejected"); }
      else if (key === "l") { event.preventDefault(); decide("needs_attention"); }
      else if (key === "u") { event.preventDefault(); undo(); }
      else if (key === "backspace") { event.preventDefault(); clearDecision(); }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [unitIndex, currentUnit, focusedId, selections, lastAction, units]);

  if (loadError) return <section className="review-shell"><p role="alert">Could not load this shoot: {loadError}</p><button className="btn" onClick={onClose}>Back to library</button></section>;
  if (queue === null) return <section className="review-shell"><div className="review-image-placeholder">Preparing local review queue…</div></section>;
  if (!currentUnit || !currentPhoto) return <section className="review-shell"><p>This shoot has no indexed photographs to review.</p><button className="btn" onClick={onClose}>Back to library</button></section>;

  const unitPhotos = currentUnit.photoIds.map((id) => photosById.get(id)).filter((photo): photo is PhotoSummary => photo !== undefined);
  const context = currentUnit.kind === "burst" ? `Burst · ${unitPhotos.length} frames` : currentUnit.kind === "similar" ? `Similar frames · ${unitPhotos.length}` : "Single frame";
  const measurement = measurementLabel(full);
  const activeSelection = focusedId === null ? null : selections[focusedId] ?? null;

  return (
    <section className="review-shell" aria-label="Shoot review">
      <header className="review-header">
        <button className="btn btn-sm" onClick={onClose}>← Library</button>
        <div><strong>Review: {sessionName}</strong><span>{context} · unit {unitIndex + 1} of {units.length}</span></div>
        <div className="review-progress" aria-label={`${counts.reviewed} of ${allPhotoIds.length} reviewed`}>
          <strong>{counts.reviewed.toLocaleString()} / {allPhotoIds.length.toLocaleString()}</strong><span>reviewed · {counts.selected} kept · {counts.needsAttention} later</span>
        </div>
      </header>

      <main className="review-stage">
        <div className="review-photo-wrap"><ReviewImage photo={currentPhoto} /></div>
        <aside className="review-details">
          <div className="mono review-filename">{currentPhoto.filename}</div>
          <p className="faint">{measurement ?? "No local measurements yet — decide from the photograph."}</p>
          {full?.capture_datetime && <p className="faint">Captured {new Date(full.capture_datetime).toLocaleString()}</p>}
          <div className="review-actions">
            <button className="btn btn-primary" onClick={() => decide("selected")}><kbd>K</kbd> Keep</button>
            <button className="btn btn-danger" onClick={() => decide("rejected")}><kbd>X</kbd> Reject</button>
            <button className="btn" onClick={() => decide("needs_attention")}><kbd>L</kbd> Later</button>
            <button className="btn btn-ghost" onClick={clearDecision} disabled={!activeSelection}><kbd>⌫</kbd> Clear</button>
          </div>
          <button className="btn btn-sm review-undo" onClick={undo} disabled={!lastAction}><kbd>U</kbd> Undo last decision</button>
          <p className="review-help">←/→ or H/J moves between moments. Decisions remain local and can be changed at any time.</p>
        </aside>
      </main>

      {unitPhotos.length > 1 && <div className="review-strip">
        {unitPhotos.slice(0, 12).map((photo) => <ReviewThumb key={photo.id} photo={photo} active={photo.id === focusedId} state={selections[photo.id] ?? null} onFocus={() => setFocusedId(photo.id)} />)}
        {unitPhotos.length > 12 && <span className="faint">+{unitPhotos.length - 12} in this group</span>}
      </div>}

      <footer className="review-footer">
        <button className="btn btn-sm" onClick={() => moveToUnit(unitIndex - 1)} disabled={unitIndex === 0}>← Previous moment</button>
        <button className="btn btn-sm" onClick={() => moveToUnit(unitIndex + 1)} disabled={unitIndex >= units.length - 1}>Next moment →</button>
        <span className="faint">No file is moved, renamed, or deleted while reviewing.</span>
      </footer>
    </section>
  );
}
