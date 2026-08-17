import { useMemo, useState } from "react";
import { api, toErrorMessage } from "@/lib/ipc";
import { useAppStore } from "@/stores/appStore";
import { usePhotos } from "@/hooks/usePhotos";
import { EmptyState } from "@/components/EmptyState";
import { ProgressBar } from "@/components/ProgressBar";
import { VirtualGrid } from "@/components/VirtualGrid";
import { PhotoTile } from "@/components/PhotoTile";
import { Viewer } from "@/features/viewer/Viewer";
import { FolderIcon } from "@/components/Icons";

export function LibraryView() {
  const activeFolder = useAppStore((s) => s.activeFolder);
  const dbStatus = useAppStore((s) => s.dbStatus);
  const scanning = useAppStore((s) => s.scanning);
  const progress = useAppStore((s) => s.progress);
  const scanSummary = useAppStore((s) => s.scanSummary);
  const store = useAppStore.getState;

  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [viewerId, setViewerId] = useState<number | null>(null);

  // Re-fetch the index whenever a scan (re)completes for this folder.
  const refreshKey = useMemo(
    () => (scanSummary && scanSummary.session_name ? scanSummary.session_name : "none"),
    [scanSummary],
  );
  const enabled = !!activeFolder && (dbStatus?.photo_count ?? 0) > 0;
  const photos = usePhotos(enabled, refreshKey);

  async function openFolder() {
    setBusy(true);
    setError(null);
    try {
      const picked = await api.pickFolder();
      if (!picked) return;
      await api.setActiveFolder(picked);
      store().setActiveFolder(picked);
      store().setScanSummary(null);
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

  return (
    <div className="library">
      <div className="library-toolbar">
        <FolderIcon size={16} />
        <span className="mono library-toolbar-path" title={activeFolder}>{activeFolder}</span>
        <span className="spacer" />
        {!scanning ? (
          <button className="btn btn-sm btn-primary" onClick={startScan} disabled={busy || photos.loading}>
            {dbStatus && dbStatus.photo_count > 0 ? "Re-scan" : "Scan folder"}
          </button>
        ) : (
          <button className="btn btn-sm btn-danger" onClick={stopScan}>Stop scan</button>
        )}
      </div>

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

      <ErrorBanner message={error ?? photos.error} />

      {!hasPhotos ? (
        <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center" }}>
          <EmptyState glyph="◫" title="Nothing indexed yet">
            <p>
              {scanning
                ? "Scanning in progress — watch the progress bar above."
                : "Press “Scan folder” to index every supported photo in this folder (JPG, PNG, WebP, TIFF, RAW, HEIC). Re-scans are safe: nothing is ever duplicated."}
            </p>
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
                render={(i) => <PhotoTile photo={photos.photos[i]} onOpen={setViewerId} />}
              />
            </div>
          )}

          <div className="library-statusbar">
            <span>{photos.total.toLocaleString()} photographs</span>
            <span className="faint">Page {photos.page + 1}</span>
            <span className="spacer" />
            <span className="faint">Local-only index · thumbnails cached on this machine</span>
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
