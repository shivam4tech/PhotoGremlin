import { useState } from "react";
import { api, toErrorMessage } from "@/lib/ipc";
import { useAppStore } from "@/stores/appStore";
import { EmptyState } from "@/components/EmptyState";
import { StatCard } from "@/components/StatCard";
import { ProgressBar } from "@/components/ProgressBar";
import { FolderIcon } from "@/components/Icons";

export function LibraryView() {
  const activeFolder = useAppStore((s) => s.activeFolder);
  const dbStatus = useAppStore((s) => s.dbStatus);
  const scanning = useAppStore((s) => s.scanning);
  const progress = useAppStore((s) => s.progress);
  const scanSummary = useAppStore((s) => s.scanSummary);
  const store = useAppStore.getState;

  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function openFolder() {
    setBusy(true);
    setError(null);
    setNotice(null);
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
    setNotice(null);
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

  return (
    <>
      {error && (
        <div
          style={{
            padding: "10px 14px",
            marginBottom: 16,
            borderRadius: 8,
            background: "var(--danger-soft)",
            color: "var(--danger)",
            fontSize: 12.5,
          }}
        >
          {error}
        </div>
      )}
      {notice && (
        <div
          style={{
            padding: "10px 14px",
            marginBottom: 16,
            borderRadius: 8,
            background: "var(--accent-soft)",
            color: "var(--accent)",
            fontSize: 12.5,
          }}
        >
          {notice}
        </div>
      )}

      {!activeFolder ? (
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
          Point PhotoGremlin at a folder of photographs. Everything — scanning,
          thumbnails, analysis, statistics — runs on this machine. Nothing is
          uploaded, ever.
        </EmptyState>
      ) : (
        <div>
          <div className="card" style={{ marginBottom: 16 }}>
            <h3>Active library</h3>
            <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
              <FolderIcon size={20} />
              <span
                className="mono"
                style={{ fontSize: 12.5, wordBreak: "break-all", flex: 1 }}
              >
                {activeFolder}
              </span>
              <button className="btn btn-sm" onClick={openFolder} disabled={busy || scanning}>
                Change
              </button>
              {!scanning ? (
                <button
                  className="btn btn-primary btn-sm"
                  onClick={startScan}
                  disabled={busy}
                >
                  {dbStatus && dbStatus.photo_count > 0 ? "Re-scan folder" : "Scan folder"}
                </button>
              ) : (
                <button className="btn btn-danger btn-sm" onClick={stopScan}>
                  Stop scan
                </button>
              )}
            </div>
          </div>

          {scanning && progress && (
            <div className="card" style={{ marginBottom: 16 }}>
              <h3>{progress.stage === "done" ? "Finishing" : "Scanning"}</h3>
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
                <div className="faint mono" style={{ fontSize: 11.5, marginTop: 8, wordBreak: "break-all" }}>
                  {progress.current}
                </div>
              )}
            </div>
          )}

          {scanSummary && !scanning && (
            <div className="card" style={{ marginBottom: 16 }}>
              <h3>Last scan — {scanSummary.session_name}</h3>
              <div className="stat-grid" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(140px, 1fr))" }}>
                <StatCard label="Indexed" value={scanSummary.indexed} sub="photographs added" />
                <StatCard label="Files in folder" value={scanSummary.total_files} />
                <StatCard label="Ignored" value={scanSummary.ignored} sub="non-photo files" />
                <StatCard
                  label="Duration"
                  value={`${(scanSummary.elapsed_ms / 1000).toFixed(1)}s`}
                  sub={scanSummary.cancelled ? "scan was stopped" : undefined}
                />
              </div>
              {scanSummary.errors.length > 0 && (
                <div
                  style={{
                    marginTop: 12,
                    fontSize: 12.5,
                    color: "var(--warning)",
                    display: "grid",
                    gap: 4,
                  }}
                >
                  {scanSummary.errors.slice(0, 5).map((e) => (
                    <div key={e}>• {e}</div>
                  ))}
                  {scanSummary.errors.length > 5 && (
                    <div className="faint">… and {scanSummary.errors.length - 5} more (see log)</div>
                  )}
                </div>
              )}
            </div>
          )}

          {dbStatus && dbStatus.photo_count > 0 ? (
            <div>
              <div className="section-title">Index status</div>
              <div className="stat-grid">
                <StatCard label="Photographs" value={dbStatus.photo_count} />
                <StatCard label="Sessions" value={dbStatus.session_count} />
                <StatCard
                  label="Analyzed"
                  value={dbStatus.analyzed_count}
                  sub={
                    dbStatus.photo_count > 0
                      ? `${Math.round((dbStatus.analyzed_count / dbStatus.photo_count) * 100)}% of library`
                      : undefined
                  }
                />
              </div>
            </div>
          ) : (
            <EmptyState glyph="◫" title="Nothing indexed yet">
              <p>
                {scanning
                  ? "Scanning in progress — watch the progress bar above."
                  : "Press “Scan folder” to index every supported photo in this folder (JPG, PNG, WebP, TIFF, RAW, HEIC). Re-scans are safe: nothing is ever duplicated."}
              </p>
            </EmptyState>
          )}
        </div>
      )}
    </>
  );
}
