import { useEffect, useMemo, useState } from "react";
import { api, toErrorMessage } from "@/lib/ipc";
import { useAppStore } from "@/stores/appStore";
import { useFilteredPhotos } from "@/hooks/useFilteredPhotos";
import { EmptyState } from "@/components/EmptyState";
import { ProgressBar } from "@/components/ProgressBar";
import { VirtualGrid } from "@/components/VirtualGrid";
import { PhotoTile } from "@/components/PhotoTile";
import { Viewer } from "@/features/viewer/Viewer";
import { FilterBar } from "@/features/library/FilterBar";
import { FileOpsPanel } from "@/features/fileops/FileOpsPanel";
import { draftToFilter } from "@/features/library/filterFields";
import { FolderIcon } from "@/components/Icons";
import type { FilterCondition } from "@/types/api";

export function LibraryView() {
  const activeFolder = useAppStore((s) => s.activeFolder);
  const dbStatus = useAppStore((s) => s.dbStatus);
  const scanning = useAppStore((s) => s.scanning);
  const progress = useAppStore((s) => s.progress);
  const scanSummary = useAppStore((s) => s.scanSummary);
  const analyzing = useAppStore((s) => s.analyzing);
  const analysisSummary = useAppStore((s) => s.analysisSummary);
  const readingMetadata = useAppStore((s) => s.readingMetadata);
  const metadataSummary = useAppStore((s) => s.metadataSummary);
  const operating = useAppStore((s) => s.operating);
  const selections = useAppStore((s) => s.selections);
  const selectionMode = useAppStore((s) => s.selectionMode);
  const libraryVersion = useAppStore((s) => s.libraryVersion);
  const store = useAppStore.getState;

  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [viewerId, setViewerId] = useState<number | null>(null);
  // Active filter as structured conditions — the exact object the engine
  // consumes (and saved views will store).
  const [filterDraft, setFilterDraft] = useState<FilterCondition[]>([]);

  // Re-fetch the index when a scan completes, a file operation changed the
  // files on disk, or the folder changed.
  const refreshKey = useMemo(
    () => `${scanSummary && scanSummary.session_name ? scanSummary.session_name : "none"}:${libraryVersion}`,
    [scanSummary, libraryVersion],
  );

  // Load persisted culling state once per library so tiles render their marks.
  useEffect(() => {
    if (selectionMode) void store().loadSelections();
  }, [selectionMode, refreshKey]);
  const libraryHasPhotos = !!activeFolder && (dbStatus?.photo_count ?? 0) > 0;
  const filterJson = useMemo(() => JSON.stringify(draftToFilter(filterDraft)), [filterDraft]);
  const photos = useFilteredPhotos(libraryHasPhotos, filterJson, refreshKey);

  async function openFolder() {
    setBusy(true);
    setError(null);
    try {
      const picked = await api.pickFolder();
      if (!picked) return;
      await api.setActiveFolder(picked);
      store().setActiveFolder(picked);
      store().setScanSummary(null);
      store().setAnalysisSummary(null);
      await store().refreshStatus();
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
    try {
      store().setMetadataSummary(null);
      await api.startMetadata();
      store().setReadingMetadata(true);
      store().setProgress({ total: 0, done: 0, stage: "reading metadata", current: null });
    } catch (e) {
      setError(toErrorMessage(e));
      store().setReadingMetadata(false);
    }
  }

  async function stopMetadata() {
    try {
      await api.stopMetadata();
    } catch (e) {
      setError(toErrorMessage(e));
    }
  }

  const anyPassRunning = scanning || analyzing || readingMetadata;
  const metadataPending = dbStatus?.metadata_pending ?? 0;

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
  const pageIds = useMemo(() => photos.photos.map((p) => p.id), [photos.photos]);

  function keep(id: number) {
    store().setSelection(id, "selected");
  }
  function reject(id: number) {
    store().setSelection(id, "rejected");
  }
  function clearSel(id: number) {
    store().setSelection(id, null);
  }

  return (
    <div className="library">
      <div className="library-toolbar">
        <FolderIcon size={16} />
        <span className="mono library-toolbar-path" title={activeFolder}>{activeFolder}</span>
        <span className="spacer" />
        {!scanning ? (
          <button className="btn btn-sm btn-primary" onClick={startScan} disabled={busy || photos.loading || anyPassRunning}>
            {dbStatus && dbStatus.photo_count > 0 ? "Re-scan" : "Scan folder"}
          </button>
        ) : (
          <button className="btn btn-sm btn-danger" onClick={stopScan}>Stop scan</button>
        )}
        {!readingMetadata && metadataPending > 0 ? (
          <button
            className="btn btn-sm"
            onClick={startMetadata}
            disabled={scanning || analyzing || busy || !libraryHasPhotos}
            title={`Read camera metadata (EXIF) from ${metadataPending.toLocaleString()} photograph${metadataPending === 1 ? "" : "s"} that still needs it.`}
          >
            Read metadata ({metadataPending.toLocaleString()})
          </button>
        ) : null}
        {readingMetadata ? (
          <button className="btn btn-sm btn-danger" onClick={stopMetadata}>
            Stop reading
          </button>
        ) : null}
        {!analyzing ? (
          <button
            className="btn btn-sm"
            onClick={startAnalysis}
            disabled={anyPassRunning || photos.total === 0 || busy}
            title="Measure every photo that still needs it: sharpness, brightness, contrast, saturation, clipping, monochrome. Re-runs are incremental."
          >
            Analyze photos
          </button>
        ) : (
          <button className="btn btn-sm btn-danger" onClick={stopAnalysis}>
            Stop analysis
          </button>
        )}
        <button
          className={`btn btn-sm${selectionMode ? " btn-primary" : ""}`}
          onClick={() => store().setSelectionMode(!selectionMode)}
          disabled={anyPassRunning || operating || photos.total === 0}
          title="Cull the library: mark photographs to keep or reject, then rename, move, copy or trash them."
        >
          {selectionMode ? "Done culling" : "Cull"}
        </button>
      </div>

      <FilterBar draft={filterDraft} onChange={setFilterDraft} disabled={anyPassRunning} />

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

      {scanSummary && !scanning && (
        <div className="library-summaryline mono">
          Last scan “{scanSummary.session_name}”: {scanSummary.indexed.toLocaleString()} indexed · {scanSummary.ignored.toLocaleString()} ignored
          {scanSummary.cancelled ? " (stopped)" : ""} · {(scanSummary.elapsed_ms / 1000).toFixed(1)}s
          {scanSummary.errors.length > 0 && (
            <span style={{ color: "var(--warning)" }}> · {scanSummary.errors.length} error{scanSummary.errors.length > 1 ? "s" : ""} in log</span>
          )}
        </div>
      )}

      {analysisSummary && !analyzing && (
        <div className="library-summaryline mono">
          Last analysis: {analysisSummary.analyzed.toLocaleString()} measured
          {analysisSummary.failed > 0 && (
            <span style={{ color: "var(--warning)" }}> · {analysisSummary.failed.toLocaleString()} failed</span>
          )}
          · {(analysisSummary.elapsed_ms / 1000).toFixed(1)}s
          {analysisSummary.cancelled ? " (stopped)" : ""}
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
                ? `${progress.done.toLocaleString()} / ${progress.total.toLocaleString()} photographs`
                : progress.stage
            }
          />
        </div>
      )}

      {metadataSummary && !readingMetadata ? (
        <div className="library-summaryline mono">
          Last metadata read: {metadataSummary.processed.toLocaleString()} photographs
          {metadataSummary.failed > 0 && (
            <span style={{ color: "var(--warning)" }}>
              {" "}· {metadataSummary.failed.toLocaleString()} unreadable
            </span>
          )}
          {" "}· {(metadataSummary.elapsed_ms / 1000).toFixed(1)}s
          {metadataSummary.cancelled ? " (stopped)" : ""}
        </div>
      ) : null}

      {selectionMode && hasPhotos && (
        <div className="cullbar">
          <span>
            {selectedIds.length.toLocaleString()} keep{selectedIds.length === 1 ? "" : "s"} · {rejectedCount.toLocaleString()} reject{rejectedCount === 1 ? "" : "ed"}
          </span>
          <span className="spacer" />
          <button
            className="btn btn-ghost btn-sm"
            onClick={() => store().setSelectionsBulk(pageIds, "selected")}
            disabled={operating || pageIds.length === 0}
            title="Mark every photograph on this page to keep"
          >
            Keep all shown
          </button>
          <button
            className="btn btn-ghost btn-sm"
            onClick={() => store().setSelectionsBulk(pageIds, null)}
            disabled={operating || pageIds.length === 0}
          >
            Clear shown
          </button>
        </div>
      )}

      {selectionMode && selectedIds.length > 0 && (
        <FileOpsPanel photoIds={selectedIds} />
      )}

      <ErrorBanner message={error ?? photos.error} />

      {!libraryHasPhotos ? (
        <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center" }}>
          <EmptyState glyph="◫" title="Nothing indexed yet">
            <p>
              {scanning
                ? "Scanning in progress — watch the progress bar above."
                : "Press “Scan folder” to index every supported photo in this folder (JPG, PNG, WebP, TIFF, RAW, HEIC). Re-scans are safe: nothing is ever duplicated."}
            </p>
          </EmptyState>
        </div>
      ) : !hasPhotos && filterDraft.length > 0 ? (
        <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center" }}>
          <EmptyState glyph="◫" title="No photographs match these filters">
            <p>
              {filterDraft.length} condition{filterDraft.length > 1 ? "s" : ""} selected ·{" "}
              {(dbStatus?.photo_count ?? 0).toLocaleString()} photographs in the library.
            </p>
            <button className="btn btn-sm" onClick={() => setFilterDraft([])}>
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
                itemCount={photos.photos.length}
                render={(i) => (
                  <PhotoTile
                    photo={photos.photos[i]}
                    onOpen={setViewerId}
                    selectionMode={selectionMode}
                    selection={selectionMode ? selections[photos.photos[i].id] ?? null : null}
                    onKeep={keep}
                    onReject={reject}
                    onClear={clearSel}
                  />
                )}
              />
            </div>
          )}

          <div className="library-statusbar">
            {filterDraft.length > 0 ? (
              <span>
                Showing {photos.total.toLocaleString()} of{" "}
                {(dbStatus?.photo_count ?? 0).toLocaleString()} photographs (
                {filterDraft.length} filter{filterDraft.length > 1 ? "s" : ""})
              </span>
            ) : (
              <span>{photos.total.toLocaleString()} photographs</span>
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
            <button className="btn btn-ghost btn-sm" onClick={photos.reload} disabled={photos.loading}>
              Refresh
            </button>
          </div>
        </>
      )}

      {viewerId !== null && (
        <Viewer
          photoId={viewerId}
          ordered={photos.photos}
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
