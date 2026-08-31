import { useEffect, useMemo, useState } from "react";
import { api, toErrorMessage } from "@/lib/ipc";
import { useAppStore, type SelectionState } from "@/stores/appStore";
import { ReviewCompareDialog } from "@/features/review/ReviewCompareDialog";
import {
  buildReviewUnits,
  comparisonPhotoIds,
  firstUnreviewedId,
  reviewCounts,
} from "@/features/review/reviewQueue";
import type { EditorConfig, PhotoFull, PhotoSummary, ReviewQueue } from "@/types/api";

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
  if (full.highlight_clipping != null && full.highlight_clipping >= 5) parts.push(`highlights ${full.highlight_clipping.toFixed(1)}%`);
  if (full.shadow_clipping != null && full.shadow_clipping >= 5) parts.push(`shadows ${full.shadow_clipping.toFixed(1)}%`);
  if ((full.closed_eye_face_count ?? 0) > 0) parts.push(`closed-eye candidate${full.max_eye_closure_confidence == null ? "" : ` ${full.max_eye_closure_confidence.toFixed(0)}%`}`);
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
  const [finishDismissed, setFinishDismissed] = useState(false);
  const [editor, setEditor] = useState<EditorConfig | null>(null);
  const [handoffBusy, setHandoffBusy] = useState(false);
  const [handoffMessage, setHandoffMessage] = useState<string | null>(null);
  const [handoffFailed, setHandoffFailed] = useState(false);
  const [compareOpen, setCompareOpen] = useState(false);

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

  useEffect(() => {
    let cancelled = false;
    api.getEditorConfig().then((config) => {
      if (!cancelled) setEditor(config);
    }).catch(() => { if (!cancelled) setEditor(null); });
    return () => { cancelled = true; };
  }, []);

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

  async function handoffKept() {
    const keptIds = allPhotoIds.filter((id) => selections[id] === "selected");
    setHandoffMessage(null);
    setHandoffFailed(false);
    if (!editor) {
      useAppStore.getState().setView("settings");
      return;
    }
    setHandoffBusy(true);
    try {
      const result = await api.launchInEditor(keptIds);
      setHandoffMessage(`Opened ${result.launched.toLocaleString()} photograph${result.launched === 1 ? "" : "s"} in ${result.application}${result.skippedMissing ? `; ${result.skippedMissing} missing file${result.skippedMissing === 1 ? " was" : "s were"} skipped` : ""}.`);
    } catch (error) {
      setHandoffFailed(true);
      setHandoffMessage(toErrorMessage(error));
    } finally {
      setHandoffBusy(false);
    }
  }

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      const target = event.target as HTMLElement | null;
      if (target?.closest("input, textarea, select, [contenteditable='true']")) return;
      if (compareOpen) return;
      const key = event.key.toLowerCase();
      if (key === "escape") { event.preventDefault(); onClose(); }
      else if (key === "arrowleft" || key === "h") { event.preventDefault(); moveToUnit(unitIndex - 1); }
      else if (key === "arrowright" || key === "j") { event.preventDefault(); moveToUnit(unitIndex + 1); }
      else if (key === "k") { event.preventDefault(); decide("selected"); }
      else if (key === "x") { event.preventDefault(); decide("rejected"); }
      else if (key === "l") { event.preventDefault(); decide("needs_attention"); }
      else if (key === "u") { event.preventDefault(); undo(); }
      else if (key === "backspace") { event.preventDefault(); clearDecision(); }
      else if (key === "c" && currentUnit && currentUnit.photoIds.length > 1) {
        event.preventDefault();
        setCompareOpen(true);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [unitIndex, currentUnit, focusedId, selections, lastAction, units, compareOpen]);

  if (loadError) return <section className="review-shell"><p role="alert">Could not load this shoot: {loadError}</p><button className="btn" onClick={onClose}>Back to library</button></section>;
  if (queue === null) return <section className="review-shell"><div className="review-image-placeholder">Preparing local review queue…</div></section>;
  if (!currentUnit || !currentPhoto) return <section className="review-shell"><p>This shoot has no indexed photographs to review.</p><button className="btn" onClick={onClose}>Back to library</button></section>;

  const unitPhotos = currentUnit.photoIds.map((id) => photosById.get(id)).filter((photo): photo is PhotoSummary => photo !== undefined);
  const context = currentUnit.kind === "burst" ? `Burst · ${unitPhotos.length} frames` : currentUnit.kind === "similar" ? `Similar frames · ${unitPhotos.length}` : "Single frame";
  const comparePhotos = comparisonPhotoIds(currentUnit.photoIds, currentPhoto.id)
    .map((id) => photosById.get(id))
    .filter((photo): photo is PhotoSummary => photo !== undefined);
  const measurement = measurementLabel(full);
  const activeSelection = focusedId === null ? null : selections[focusedId] ?? null;
  const finished = allPhotoIds.length > 0 && counts.reviewed === allPhotoIds.length;

  if (finished && !finishDismissed) {
    const keptIds = allPhotoIds.filter((id) => selections[id] === "selected");
    const launchTooLarge = Boolean(editor && keptIds.length > editor.maxFilesPerLaunch);
    return (
      <section className="review-shell" aria-label="Shoot review complete">
        <header className="review-header">
          <button className="btn btn-sm" onClick={onClose}>← Library</button>
          <div><strong>Review: {sessionName}</strong><span>All indexed photographs have a decision</span></div>
          <div className="review-progress"><strong>{allPhotoIds.length.toLocaleString()} / {allPhotoIds.length.toLocaleString()}</strong><span>reviewed</span></div>
        </header>
        <main className="review-finish">
          <div className="review-finish-card">
            <span className="review-finish-kicker">Shoot review complete</span>
            <h2>Review decisions are complete.</h2>
            <p>Every decision is stored locally and remains reversible. Source photographs have not been moved, renamed, changed, or deleted.</p>
            <div className="review-finish-counts" aria-label="Review totals">
              <div><strong>{counts.selected.toLocaleString()}</strong><span>kept</span></div>
              <div><strong>{counts.rejected.toLocaleString()}</strong><span>rejected</span></div>
              <div><strong>{counts.needsAttention.toLocaleString()}</strong><span>later</span></div>
            </div>
            <div className="review-finish-actions">
              <button className="btn btn-primary" disabled={handoffBusy || keptIds.length === 0 || launchTooLarge} onClick={() => void handoffKept()}>
                {editor ? `Open kept in ${editor.displayName}` : "Configure editing app…"}
              </button>
              <button className="btn" onClick={() => {
                useAppStore.getState().setFilterConditions([{ field: "review_state", operator: "=", value: "selected" }]);
                onClose();
              }}>Return to kept set</button>
              <button className="btn btn-ghost" onClick={() => setFinishDismissed(true)}>Review decisions</button>
            </div>
            {launchTooLarge && <p className="review-finish-note">This kept set exceeds the {editor?.maxFilesPerLaunch} file direct-launch limit. Return to the kept set and use Export originals.</p>}
            {handoffMessage && <p role={handoffFailed ? "alert" : "status"} className={handoffFailed ? "review-finish-error" : "review-finish-status"}>{handoffMessage}</p>}
          </div>
        </main>
        <footer className="review-footer">
          <button className="btn btn-sm review-undo" onClick={undo} disabled={!lastAction}><kbd>U</kbd> Undo last decision</button>
          <span className="faint">No file was changed during review.</span>
        </footer>
      </section>
    );
  }

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
          {unitPhotos.length > 1 && (
            <button className="btn btn-sm review-compare-trigger" onClick={() => setCompareOpen(true)}>
              <kbd>C</kbd> Compare up to {Math.min(4, unitPhotos.length)} frames
            </button>
          )}
          <button className="btn btn-sm review-undo" onClick={undo} disabled={!lastAction}><kbd>U</kbd> Undo last decision</button>
          <p className="review-help">←/→ or H/J moves between moments. C compares a sequence. Decisions remain local and can be changed at any time.</p>
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

      {compareOpen && comparePhotos.length > 1 && (
        <ReviewCompareDialog
          photos={comparePhotos}
          focusedId={currentPhoto.id}
          selections={selections}
          onFocus={setFocusedId}
          onClose={() => setCompareOpen(false)}
        />
      )}
    </section>
  );
}
