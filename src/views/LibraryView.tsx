import { useEffect, useMemo, useRef, useState } from "react";
import { api, toErrorMessage } from "@/lib/ipc";
import { useAppStore } from "@/stores/appStore";
import { useFilteredPhotos } from "@/hooks/useFilteredPhotos";
import { EmptyState } from "@/components/EmptyState";
import { ProgressBar } from "@/components/ProgressBar";
import { VirtualGrid } from "@/components/VirtualGrid";
import { PhotoTile } from "@/components/PhotoTile";
import { Viewer } from "@/features/viewer/Viewer";
import { FilterBar } from "@/features/library/FilterBar";
import { CullActionTray } from "@/features/library/CullActionTray";
import { ReviewMode } from "@/features/review/ReviewMode";
import { cleanName } from "@/features/organize/labels";
import { draftToFilter } from "@/features/library/filterFields";
import { FolderIcon } from "@/components/Icons";

export function LibraryView() {
  const activeFolder = useAppStore((s) => s.activeFolder);
  const dbStatus = useAppStore((s) => s.dbStatus);
  const scanning = useAppStore((s) => s.scanning);
  const progress = useAppStore((s) => s.progress);
  const scanSummary = useAppStore((s) => s.scanSummary);
  const analyzing = useAppStore((s) => s.analyzing);
  const readingMetadata = useAppStore((s) => s.readingMetadata);
  const metadataPaused = useAppStore((s) => s.metadataPaused);
  const operating = useAppStore((s) => s.operating);
  const selections = useAppStore((s) => s.selections);
  const selectionMode = useAppStore((s) => s.selectionMode);
  const libraryVersion = useAppStore((s) => s.libraryVersion);
  const filterConditions = useAppStore((s) => s.filterConditions);
  const collections = useAppStore((s) => s.collections);
  const findingSimilar = useAppStore((s) => s.findingSimilar);
  const similarityProgress = useAppStore((s) => s.similarityProgress);
  const marksVersion = useAppStore((s) => s.marksVersion);
  const store = useAppStore.getState;

  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [viewerId, setViewerId] = useState<number | null>(null);
  const [reviewMode, setReviewMode] = useState(false);

  // Saving the current filter as a named view.
  const [saveViewOpen, setSaveViewOpen] = useState(false);
  const [viewName, setViewName] = useState("");

  // Re-fetch the index when a scan completes, a file operation changed the
  // files on disk, or the folder changed. Include activeFolder so switching
  // projects immediately shows the new project's photos.
  const refreshKey = useMemo(
    () => `${activeFolder ?? "none"}:${scanSummary && scanSummary.session_name ? scanSummary.session_name : "none"}:${libraryVersion}:${marksVersion}`,
    [activeFolder, scanSummary, libraryVersion, marksVersion],
  );

  const [sessionId, setSessionId] = useState<number | null>(null);
  const [sessionPhotoCount, setSessionPhotoCount] = useState<number | null>(null);
  const [sessionName, setSessionName] = useState("This shoot");

  // Resolve the current project's session so the grid shows only its photos.
  useEffect(() => {
    if (!activeFolder) { setSessionId(null); setSessionPhotoCount(null); setSessionName("This shoot"); return; }
    let cancelled = false;
    api.listSessions().then((rows) => {
      if (cancelled) return;
      const hit = rows.find((r) => r.root_path === activeFolder);
      setSessionId(hit ? hit.id : null);
      setSessionPhotoCount(hit ? hit.photo_count : null);
      setSessionName(hit?.name ?? "This shoot");
    }).catch(() => { if (!cancelled) { setSessionId(null); setSessionPhotoCount(null); setSessionName("This shoot"); } });
    return () => { cancelled = true; };
  }, [activeFolder, scanSummary, dbStatus?.photo_count]);

  // Cursor-paged and session-scoped: a large catalog is never silently
  // truncated and another project's decisions cannot enter this view.
  useEffect(() => {
    if (selectionMode && (!activeFolder || sessionId !== null)) {
      void store().loadSelections(sessionId);
    }
  }, [selectionMode, refreshKey, sessionId, activeFolder]);

  const libraryHasPhotos = !!activeFolder && (dbStatus?.photo_count ?? 0) > 0;
  const filterJson = useMemo(() => {
    const base = draftToFilter(filterConditions);
    // When the folder has no session yet (before first scan), show nothing
    // rather than leaking in photos from other projects.
    if (sessionId === null && activeFolder) {
      return JSON.stringify({ operator: "AND", conditions: [{ field: "session_id", operator: "=", value: -1 }] });
    }
    if (sessionId === null) return JSON.stringify(base);
    const sessionCond = { field: "session_id", operator: "=", value: sessionId };
    if (typeof base === "string" && base === "") {
      return JSON.stringify({ operator: "AND", conditions: [sessionCond] });
    }
    const obj = base as { operator: string; conditions: unknown[] };
    return JSON.stringify({ ...obj, conditions: [...obj.conditions, sessionCond] });
  }, [filterConditions, sessionId, activeFolder]);
  const photos = useFilteredPhotos(
    libraryHasPhotos,
    filterJson,
    refreshKey,
  );

  // Publish live counts for the TopBar badge: filtered vs session total.
  // Group view is handled separately (its own count in the statusbar).
  const setCurrentViewCount = useAppStore((s) => s.setCurrentViewCount);
  useEffect(() => {
    if (!activeFolder) {
      setCurrentViewCount(null, null);
      return;
    }
    // photos.total is session-scoped filtered count; sessionPhotoCount is the
    // unfiltered session total (null before the session row loads — fall back
    // to filtered total so the badge never shows 0 while loading).
    setCurrentViewCount(photos.total, sessionPhotoCount ?? photos.total);
    return () => { setCurrentViewCount(null, null); };
  }, [activeFolder, photos.total, sessionPhotoCount, setCurrentViewCount]);

  const anyPassRunning = scanning || analyzing || readingMetadata;
  const metadataPending = dbStatus?.metadata_pending ?? 0;

  async function openFolder() {
    setBusy(true);
    setError(null);
    try {
      // The store action is shared with the ⌘/Ctrl+O shortcut (Sprint 10).
      await store().openFolder();
    } catch (e) {
      setError(toErrorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function startScan() {
    if (!activeFolder) return;
    setError(null);
    try {
      await api.startScan(activeFolder);
      store().setScanning(true);
      store().setProgress({ total: 0, done: 0, stage: "discovering", current: null });
      store().setScanSummary(null);
    } catch (e) {
      setError(toErrorMessage(e));
      store().setScanning(false);
    }
  }

  async function stopScan() {
    try {
      await api.stopScan();
    } catch (e) {
      setError(toErrorMessage(e));
    }
  }

  async function startAnalysis() {
    setError(null);
    try {
      await api.startAnalysis();
      store().setAnalyzing(true);
      store().setProgress({ total: 0, done: 0, stage: "analyzing", current: null });
      store().setAnalysisSummary(null);
    } catch (e) {
      setError(toErrorMessage(e));
      store().setAnalyzing(false);
    }
  }

  async function stopAnalysis() {
    try {
      await api.stopAnalysis();
    } catch (e) {
      setError(toErrorMessage(e));
    }
  }

  async function startMetadata() {
    setError(null);
    // Set the visible pending state before invoking Rust. The task can emit
    // its first real progress event before invoke() resolves; assigning 0/0
    // afterwards would overwrite it and make a healthy pass look stuck.
    store().setReadingMetadata(true);
    try {
      store().setMetadataSummary(null);
      store().setMetadataPaused(false);
      store().setProgress({ total: 0, done: 0, stage: "reading metadata", current: null });
      await api.startMetadata();
    } catch (e) {
      setError(toErrorMessage(e));
      store().setReadingMetadata(false);
      store().setMetadataPaused(false);
      store().setProgress(null);
    }
  }

  async function stopMetadata() {
    try {
      const stopping = await api.stopMetadata();
      if (stopping) {
        store().setMetadataPaused(false);
        const progress = useAppStore.getState().progress;
        store().setProgress({
          total: progress?.total ?? 0,
          done: progress?.done ?? 0,
          stage: "stopping metadata",
          current: progress?.current ?? null,
        });
      }
    } catch (e) {
      setError(toErrorMessage(e));
    }
  }

  async function toggleMetadataPause() {
    try {
      const changed = metadataPaused ? await api.resumeMetadata() : await api.pauseMetadata();
      if (!changed) return;
      const nextPaused = !metadataPaused;
      store().setMetadataPaused(nextPaused);
      const progress = useAppStore.getState().progress;
      store().setProgress({
        total: progress?.total ?? 0,
        done: progress?.done ?? 0,
        stage: nextPaused ? "metadata paused" : "reading metadata",
        current: progress?.current ?? null,
      });
    } catch (e) {
      setError(toErrorMessage(e));
    }
  }

  async function saveCurrentView() {
    const name = cleanName(viewName);
    if (name === null) {
      setError("View name must be 1–60 characters.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await api.saveView(name, filterJson, null);
      setViewName("");
      setSaveViewOpen(false);
      await store().loadSavedViews();
      store().setNotice(`Saved view “${name}” — reopen it from Saved Views.`);
    } catch (e) {
      setError(toErrorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function startSimilarity() {
    setError(null);
    try {
      store().setSimilaritySummary(null);
      store().setSimilarityProgress({ total: 0, done: 0, stage: "hashing", current: null });
      await api.startSimilarity();
      store().setFindingSimilar(true);
    } catch (e) {
      setError(toErrorMessage(e));
      store().setFindingSimilar(false);
      store().setSimilarityProgress(null);
    }
  }

  async function stopSimilarity() {
    try {
      await api.stopSimilarity();
    } catch (e) {
      setError(toErrorMessage(e));
    }
  }

  const hasPhotos = photos.total > 0;

  // Culling: ids marked "selected" drive the file-operations panel.
  const selectedIds = useMemo(
    () => Object.keys(selections).filter((k) => selections[Number(k)] === "selected").map(Number),
    [selections],
  );
  const rejectedCount = useMemo(
    () => Object.values(selections).filter((s) => s === "rejected").length,
    [selections],
  );
  const laterCount = useMemo(
    () => Object.values(selections).filter((s) => s === "needs_attention").length,
    [selections],
  );
  const pagePhotos = photos.photos;
  const pageIds = useMemo(() => pagePhotos.map((p) => p.id), [pagePhotos]);

  async function addToCollection(collectionId: number) {
    if (selectedIds.length === 0) return;
    setError(null);
    try {
      const added = await api.addToCollection(collectionId, selectedIds);
      await store().loadCollections();
      store().setNotice(
        added === 0
          ? "Those photographs are already in that collection."
          : `Added ${added.toLocaleString()} photograph${added === 1 ? "" : "s"} to the collection.`,
      );
    } catch (e) {
      setError(toErrorMessage(e));
    }
  }

  function keep(id: number) {
    store().setSelection(id, "selected");
  }
  function reject(id: number) {
    store().setSelection(id, "rejected");
  }
  function clearSel(id: number) {
    store().setSelection(id, null);
  }

  // Forward wheel events from toolbar/filter areas to the photo grid
  function forwardWheel(e: React.WheelEvent) {
    const vg = document.querySelector(".vg") as HTMLDivElement | null;
    if (vg && e.deltaY !== 0) {
      const atTop = vg.scrollTop <= 0;
      const atBottom = vg.scrollTop + vg.clientHeight >= vg.scrollHeight - 1;
      if ((e.deltaY < 0 && !atTop) || (e.deltaY > 0 && !atBottom)) {
        vg.scrollTop += e.deltaY;
        e.preventDefault();
      }
    }
  }

  // Reset scroll when switching projects
  const prevFolderRef = useRef(activeFolder);
  useEffect(() => {
    if (prevFolderRef.current !== activeFolder) {
      prevFolderRef.current = activeFolder;
      setReviewMode(false);
      const vg = document.querySelector(".vg") as HTMLDivElement | null;
      if (vg) vg.scrollTop = 0;
    }
  }, [activeFolder]);

  if (!activeFolder) {
    return (
      <>
        <ErrorBanner message={error} />
        <EmptyState
          glyph={<FolderIcon size={40} />}
          title="Open a photo folder"
          action={
            <button className="btn btn-primary" onClick={openFolder} disabled={busy}>
              <FolderIcon size={16} />
              {busy ? "Opening…" : "Open Folder"}
            </button>
          }
        >
          Point PhotoGremlin at a folder of photographs. Everything —
          scanning, thumbnails, analysis, statistics — runs on this machine.
          Nothing is uploaded, ever.
        </EmptyState>
      </>
    );
  }

  if (reviewMode && sessionId !== null) {
    return <ReviewMode sessionId={sessionId} sessionName={sessionName} onClose={() => setReviewMode(false)} />;
  }

  return (
    <div className="library" onWheel={forwardWheel}>
      <div className="library-toolbar">
        <FolderIcon size={16} />
        <span className="mono library-toolbar-path" title={activeFolder}>{activeFolder}</span>
        <span className="spacer" />
        {!scanning ? (
          <button
            className="btn btn-sm btn-primary"
            onClick={startScan}
            disabled={busy || photos.loading || anyPassRunning || findingSimilar}
          >
            {dbStatus && dbStatus.photo_count > 0 ? "Re-scan" : "Scan folder"}
          </button>
        ) : (
          <button className="btn btn-sm btn-danger" onClick={stopScan}>Stop scan</button>
        )}
        {!readingMetadata && metadataPending > 0 ? (
          <button
            className="btn btn-sm"
            onClick={startMetadata}
            disabled={scanning || analyzing || findingSimilar || busy || !libraryHasPhotos}
            title={`Camera metadata is read automatically after each scan. Retry the ${metadataPending.toLocaleString()} pending photograph${metadataPending === 1 ? "" : "s"} only if a previous read was stopped or files changed on disk.`}
          >
            Retry metadata ({metadataPending.toLocaleString()})
          </button>
        ) : null}
        {readingMetadata ? (
          <>
            <button className="btn btn-sm" onClick={() => void toggleMetadataPause()}>
              {metadataPaused ? "Resume metadata" : "Pause metadata"}
            </button>
            <button className="btn btn-sm btn-danger" onClick={() => void stopMetadata()}>
              Stop reading
            </button>
          </>
        ) : null}
        {!analyzing ? (
          <button
            className="btn btn-sm"
            onClick={startAnalysis}
            disabled={anyPassRunning || findingSimilar || photos.total === 0 || busy}
            title="Measure every photo that still needs it: sharpness, brightness, contrast, saturation, clipping, monochrome. Re-runs are incremental."
          >
            Analyze photos
          </button>
        ) : (
          <button className="btn btn-sm btn-danger" onClick={stopAnalysis}>
            Stop analysis
          </button>
        )}
        {!findingSimilar ? (
          <button
            className="btn btn-sm"
            onClick={() => void startSimilarity()}
            disabled={anyPassRunning || operating || photos.total === 0 || busy}
            title="Photograph every image with a perceptual hash, then group near-duplicates (similar) and same-moment runs (bursts). Re-runs only process changed or new files."
          >
            Find similar photos
          </button>
        ) : (
          <button className="btn btn-sm btn-danger" onClick={() => void stopSimilarity()}>
            Stop similarity
          </button>
        )}
        <button
          className={`btn btn-sm${selectionMode ? " btn-primary" : ""}`}
          onClick={() => store().setSelectionMode(!selectionMode)}
          disabled={anyPassRunning || operating || findingSimilar || photos.total === 0}
          title="Cull the library: mark photographs to keep or reject, then rename, move, copy or trash them."
        >
          {selectionMode ? "Done culling" : "Cull"}
        </button>
        {sessionId !== null && (
          <button
            className="btn btn-sm btn-primary"
            onClick={() => setReviewMode(true)}
            disabled={anyPassRunning || operating || findingSimilar || photos.total === 0}
            title="Review this shoot in capture-time order, with burst and similar-frame context. No files are changed."
          >
            Review this shoot
          </button>
        )}
      </div>

      <div className="library-workspace">
        <section className="library-stage" aria-label="Photograph workspace">
      {scanning && progress && (
        <div className="library-scanline">
          <ProgressBar
            value={progress.done}
            max={progress.total}
            label={
              progress.total > 0
                ? `${progress.done.toLocaleString()} / ${progress.total.toLocaleString()} files`
                : progress.stage
            }
          />
          {progress.current && (
            <div className="faint mono" style={{ fontSize: 11, marginTop: 4, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              {progress.current}
            </div>
          )}
        </div>
      )}

      {analyzing && progress && (
        <div className="library-scanline">
          <ProgressBar
            value={progress.done}
            max={progress.total}
            label={
              progress.total > 0
                ? `${progress.done.toLocaleString()} / ${progress.total.toLocaleString()} photographs`
                : progress.stage
            }
          />
          {progress.current && (
            <div className="faint mono" style={{ fontSize: 11, marginTop: 4, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              {progress.current}
            </div>
          )}
        </div>
      )}

      {readingMetadata && progress && (
        <div className="library-scanline">
          <ProgressBar
            value={progress.done}
            max={progress.total}
            label={
              progress.total > 0
                ? `${progress.done.toLocaleString()} / ${progress.total.toLocaleString()} photographs — ${progress.stage}`
                : progress.stage
            }
          />
          {progress.current && (
            <div className="faint mono" style={{ fontSize: 11, marginTop: 4, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              {progress.current}
            </div>
          )}
        </div>
      )}

      {findingSimilar && similarityProgress && (
        <div className="library-scanline">
          <ProgressBar
            value={similarityProgress.done}
            max={similarityProgress.total}
            label={
              similarityProgress.total > 0
                ? `Hashing ${similarityProgress.done.toLocaleString()} / ${similarityProgress.total.toLocaleString()} photographs`
                : similarityProgress.stage
            }
          />
          {similarityProgress.current && (
            <div className="faint mono" style={{ fontSize: 11, marginTop: 4, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              {similarityProgress.current}
            </div>
          )}
        </div>
      )}

      {selectionMode && hasPhotos && (
        <CullActionTray
          selectedIds={selectedIds}
          rejectedCount={rejectedCount}
          laterCount={laterCount}
          shownCount={pageIds.length}
          operating={operating}
          collections={collections ?? []}
          onAddToCollection={addToCollection}
          onKeepAllShown={() => store().setSelectionsBulk(pageIds, "selected")}
          onClearShown={() => store().setSelectionsBulk(pageIds, null)}
        />
      )}

      <ErrorBanner message={error ?? photos.error} />

      {!libraryHasPhotos ? (
        <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center" }}>
          <EmptyState glyph="◫" title="Nothing indexed yet">
            <p>
              {scanning ? (
                "Scanning in progress — watch the progress bar above."
              ) : scanSummary ? (
                scanSummary.indexed === 0
                  ? "The scan finished, but found no supported photos in this folder. JPG, PNG, WebP and TIFF are decoded; RAW previews are rendered locally where the camera format is supported; HEIC files are indexed without a local preview. You can point at another folder with “Open folder”."
                  : "This folder no longer has indexed photographs — they may have been trashed or moved. Re-scan to pick up anything new."
              ) : (
                "Press “Scan folder” to index every supported photo in this folder (JPG, PNG, WebP, TIFF, RAW, HEIC). Re-scans are safe: nothing is ever duplicated."
              )}
            </p>
            {!scanning && (
              <button className="btn btn-primary btn-sm" onClick={openFolder}>
                Open folder
              </button>
            )}
          </EmptyState>
        </div>
      ) : !hasPhotos && filterConditions.length > 0 ? (
        <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center" }}>
          <EmptyState glyph="◫" title="No photographs match these filters">
            <p>
              {filterConditions.length} condition{filterConditions.length > 1 ? "s" : ""} selected ·{" "}
              {(dbStatus?.photo_count ?? 0).toLocaleString()} photographs in the library.
            </p>
            <button className="btn btn-sm" onClick={() => store().setFilterConditions([])}>
              Clear filters
            </button>
          </EmptyState>
        </div>
      ) : (
        <>
          {photos.loading && photos.photos.length === 0 ? (
            <div className="library-loading">Loading library…</div>
          ) : (
            <div className="library-grid-area">
              <VirtualGrid
                itemCount={pagePhotos.length}
                onReachEnd={() => { if (photos.hasMore && !photos.loading) photos.loadMore(); }}
                render={(i) => (
                  <PhotoTile
                    photo={pagePhotos[i]}
                    onOpen={setViewerId}
                    selectionMode={selectionMode}
                    selection={selectionMode ? selections[pagePhotos[i].id] ?? null : null}
                    onKeep={keep}
                    onReject={reject}
                    onClear={clearSel}
                    marksMode={selectionMode ? "always" : "contextual"}
                  />
                )}
              />
            </div>
          )}

          <div className="library-statusbar">
            {filterConditions.length > 0 ? (
              <span>
                Showing {pagePhotos.length.toLocaleString()} of {photos.total.toLocaleString()} filtered
                {photos.hasMore ? " — scroll for more" : ""} · {filterConditions.length} filter{filterConditions.length > 1 ? "s" : ""}
              </span>
            ) : (
              <span>
                {pagePhotos.length.toLocaleString()} of {photos.total.toLocaleString()} photographs
                {photos.hasMore ? " — scroll for more" : ""}
              </span>
            )}
            <span className="faint">Page {photos.page + 1}</span>
            {dbStatus && dbStatus.analyzed_count > 0 && (
              <span className="faint">{dbStatus.analyzed_count.toLocaleString()} analyzed</span>
            )}
            {metadataPending > 0 && (
              <span className="faint">{metadataPending.toLocaleString()} awaiting metadata</span>
            )}
            <span className="spacer" />
            <span className="faint">Local-only index · thumbnails &amp; analysis on this machine</span>
            <button className="btn btn-ghost btn-sm" onClick={photos.reload} disabled={photos.loading}>Refresh</button>
          </div>
        </>
      )}

        </section>

        <aside className="library-inspector" aria-label="Photo filters">
          <div className="library-inspector-head">
            <div>
              <strong>Filters</strong>
              <span className="faint">Refine this project</span>
            </div>
            {filterConditions.length > 0 && (
              <button
                className="btn btn-ghost btn-sm"
                onClick={() => store().setFilterConditions([])}
              >
                Clear {filterConditions.length}
              </button>
            )}
          </div>

          <>
              <FilterBar
                mode="inspector"
                draft={filterConditions}
                onChange={(conditions) => store().setFilterConditions(conditions)}
                disabled={anyPassRunning}
                sessionId={sessionId}
              />

              {filterConditions.length > 0 && (
                <div className="inspector-save">
                  {!saveViewOpen ? (
                    <button
                      className="btn btn-sm"
                      onClick={() => setSaveViewOpen(true)}
                      title="Save this filter with a name; it stays dynamic as the library changes."
                    >
                      Save as view
                    </button>
                  ) : (
                    <div className="inspector-save-form">
                      <input
                        className="input"
                        placeholder="View name…"
                        value={viewName}
                        autoFocus
                        onChange={(event) => setViewName(event.target.value)}
                        onKeyDown={(event) => {
                          if (event.key === "Enter") void saveCurrentView();
                          if (event.key === "Escape") setSaveViewOpen(false);
                        }}
                        aria-label="Saved view name"
                      />
                      <button
                        className="btn btn-sm btn-primary"
                        onClick={() => void saveCurrentView()}
                        disabled={busy}
                      >
                        Save
                      </button>
                      <button className="btn btn-ghost btn-sm" onClick={() => setSaveViewOpen(false)}>
                        Cancel
                      </button>
                    </div>
                  )}
                </div>
              )}
          </>
        </aside>
      </div>

      {viewerId !== null && (
        <Viewer
          photoId={viewerId}
          ordered={pagePhotos}
          onClose={() => setViewerId(null)}
          onNavigate={setViewerId}
        />
      )}
    </div>
  );
}

function ErrorBanner({ message }: { message: string | null }) {
  if (!message) return null;
  return (
    <div
      role="alert"
      style={{
        margin: "0",
        padding: "7px 32px",
        background: "var(--danger-soft)",
        color: "var(--danger)",
        fontSize: 12.5,
      }}
    >
      {message}
    </div>
  );
}
