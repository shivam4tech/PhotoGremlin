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
      await store().refreshStatus();
      setNotice("Folder set as your library. The scanner goes live in the next update — indexed photos appear below as they are ingested.");
    } catch (e) {
      setError(toErrorMessage(e));
    } finally {
      setBusy(false);
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
          glyph={<span style={{ display: "inline-flex" }}><FolderIcon size={40} /></span>}
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
              <span className="mono" style={{ fontSize: 12.5, wordBreak: "break-all", flex: 1 }}>
                {activeFolder}
              </span>
              <button className="btn btn-sm" onClick={openFolder} disabled={busy}>
                Change
              </button>
            </div>
          </div>

          {scanning && progress && (
            <div className="card" style={{ marginBottom: 16 }}>
              <h3>Scanning</h3>
              <ProgressBar
                value={progress.done}
                max={progress.total}
                label={
                  progress.current
                    ? `${progress.stage}: ${progress.current}`
                    : `${progress.stage} — ${progress.done.toLocaleString()} / ${progress.total.toLocaleString()}`
                }
              />
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
                The photo scanner is being wired up next. Once it runs, thousands of
                photos will index in seconds, and their thumbnails, technical measurements
                and statistics will appear here.
              </p>
            </EmptyState>
          )}
        </div>
      )}
    </>
  );
}
