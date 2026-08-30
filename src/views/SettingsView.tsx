import { useEffect, useState } from "react";
import { api, toErrorMessage } from "@/lib/ipc";
import { useAppStore } from "@/stores/appStore";
import { MoonIcon, ShieldIcon, SunIcon } from "@/components/Icons";
import {
  formatFaceSummaryLine,
  formatFacesProgressLine,
  formatScenesProgressLine,
  formatModelSize,
  runtimeLine,
} from "@/features/settings/ai";
import { SHORTCUTS } from "@/features/shortcuts";
import type { CacheStatus, CatalogHealth, EditorConfig } from "@/types/api";

function PathRow({ label, value }: { label: string; value: string }) {
  return (
    <div style={{ display: "flex", gap: 14, padding: "8px 0", borderBottom: "1px solid var(--border-subtle)" }}>
      <div style={{ width: 150, flexShrink: 0 }} className="faint">
        {label}
      </div>
      <div className="mono" style={{ fontSize: 12, wordBreak: "break-all" }}>
        {value}
      </div>
    </div>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.max(0, bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function StorageMaintenanceCard() {
  const [cache, setCache] = useState<CacheStatus | null>(null);
  const [health, setHealth] = useState<CatalogHealth | null>(null);
  const [backups, setBackups] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  async function refresh() {
    const [nextCache, nextHealth, nextBackups] = await Promise.all([
      api.cacheStatus(),
      api.catalogHealth(),
      api.listCatalogBackups(),
    ]);
    setCache(nextCache);
    setHealth(nextHealth);
    setBackups(nextBackups);
  }

  useEffect(() => {
    void refresh().catch((error) => setMessage(toErrorMessage(error)));
  }, []);

  async function run(action: () => Promise<unknown>, success: string) {
    setBusy(true);
    setMessage(null);
    try {
      await action();
      await refresh();
      setMessage(success);
    } catch (error) {
      setMessage(toErrorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="card" style={{ marginBottom: 16 }}>
      <h3>Catalog safety &amp; preview cache</h3>
      <p className="faint" style={{ marginTop: 0 }}>
        Each project has an isolated local catalog. Backups contain catalog data and decisions, never copies of source photographs.
      </p>
      <div className="stat-grid" style={{ marginBottom: 12 }}>
        <div className="stat-card">
          <div className="label">Catalog</div>
          <div className="value" style={{ fontSize: 16 }}>{health?.healthy ? "Healthy" : "Checking…"}</div>
          {health && <div className="faint" style={{ fontSize: 12 }}>schema {health.schemaVersion}</div>}
        </div>
        <div className="stat-card">
          <div className="label">Preview cache</div>
          <div className="value" style={{ fontSize: 16 }}>{cache ? formatBytes(cache.bytes) : "Checking…"}</div>
          {cache && <div className="faint" style={{ fontSize: 12 }}>{cache.files.toLocaleString()} files</div>}
        </div>
      </div>
      <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
        <button className="btn btn-sm" disabled={busy} onClick={() => void run(api.backupCatalog, "Catalog backup created.")}>Back up catalog</button>
        <label className="faint" style={{ display: "flex", gap: 6, alignItems: "center", fontSize: 12.5 }}>
          Cache limit
          <select
            className="input"
            disabled={busy || !cache}
            value={cache ? Math.round(cache.quota_bytes / (1024 ** 3)) : 5}
            onChange={(event) => void run(
              () => api.setCacheQuota(Number(event.target.value) * 1024 ** 3),
              "Preview cache limit updated.",
            )}
            style={{ width: 90 }}
          >
            {[1, 5, 10, 25, 50].map((gb) => <option key={gb} value={gb}>{gb} GB</option>)}
          </select>
        </label>
        <button className="btn btn-sm" disabled={busy || !cache || cache.files === 0} onClick={() => void run(api.clearCache, "Preview cache cleared. It will rebuild as needed.")}>Clear previews</button>
      </div>
      {backups.length > 0 && (
        <details style={{ marginTop: 12 }}>
          <summary className="faint" style={{ cursor: "pointer", fontSize: 12.5 }}>Restore from a local backup</summary>
          <div style={{ display: "grid", gap: 6, marginTop: 8 }}>
            {backups.slice(0, 5).map((path) => (
              <div key={path} style={{ display: "flex", gap: 8, alignItems: "center" }}>
                <span className="mono faint" style={{ fontSize: 11.5, flex: 1, wordBreak: "break-all" }}>{path.split(/[\\/]/).pop()}</span>
                <button className="btn btn-sm" disabled={busy} onClick={() => {
                  if (!window.confirm("Restore this catalog backup? PhotoGremlin will switch to a recovered copy; your current catalog and photographs will not be overwritten.")) return;
                  void run(async () => { await api.restoreCatalog(path); window.location.reload(); }, "Catalog restored.");
                }}>Restore copy</button>
              </div>
            ))}
          </div>
        </details>
      )}
      {message && <p role="status" className="faint" style={{ marginBottom: 0, fontSize: 12.5 }}>{message}</p>}
    </div>
  );
}

function EditingApplicationCard() {
  const [config, setConfig] = useState<EditorConfig | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let cancelled = false;
    api.getEditorConfig()
      .then((value) => { if (!cancelled) setConfig(value); })
      .catch((error) => { if (!cancelled) { setMessage(toErrorMessage(error)); setFailed(true); } })
      .finally(() => { if (!cancelled) setLoaded(true); });
    return () => { cancelled = true; };
  }, []);

  async function choose() {
    setBusy(true);
    setMessage(null);
    setFailed(false);
    try {
      const executable = await api.pickEditorApplication();
      if (!executable) return;
      const next = await api.setEditorConfig(executable);
      setConfig(next);
      setMessage(`${next.displayName} is ready for local handoff.`);
    } catch (error) {
      setFailed(true);
      setMessage(toErrorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  async function clear() {
    setBusy(true);
    setMessage(null);
    setFailed(false);
    try {
      await api.clearEditorConfig();
      setConfig(null);
      setMessage("Editing application cleared.");
    } catch (error) {
      setFailed(true);
      setMessage(toErrorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="card settings-card">
      <h3>Editing handoff</h3>
      <p className="faint settings-card-copy">
        Send a kept set to Lightroom, Capture One, darktable, or another desktop editor. PhotoGremlin starts the application with your source files; it never writes to an editor catalog or changes the photographs.
      </p>
      <div className="editor-config-row">
        <div className="editor-config-summary">
          <span className="label">Editing application</span>
          <strong>{!loaded ? "Checking…" : config?.displayName ?? "Not configured"}</strong>
          {config && <span className="mono faint">{config.executable}</span>}
        </div>
        <div className="settings-actions">
          <button className="btn btn-sm" disabled={busy} onClick={() => void choose()}>
            {config ? "Change…" : "Choose application…"}
          </button>
          {config && <button className="btn btn-sm btn-ghost" disabled={busy} onClick={() => void clear()}>Clear</button>}
        </div>
      </div>
      <p className="faint settings-note">
        Direct launch is capped at {config?.maxFilesPerLaunch ?? 500} files for stability. Use Export originals for larger selections.
      </p>
      {message && <p role={failed ? "alert" : "status"} className={failed ? "settings-inline-error" : "settings-inline-status"}>{message}</p>}
    </div>
  );
}

/**
 * Local intelligence (Sprint 9): optional face detection, entirely local.
 * Off by default; the card is the only place it is configured.
 */
function LocalIntelligenceCard() {
  const aiStatus = useAppStore((s) => s.aiStatus);
  const aiEnabled = useAppStore((s) => s.aiEnabled);
  const detectingFaces = useAppStore((s) => s.detectingFaces);
  const facesProgress = useAppStore((s) => s.facesProgress);
  const facesSummary = useAppStore((s) => s.facesSummary);
  const error = useAppStore((s) => s.error);

  const store = () => useAppStore.getState();

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        await useAppStore.getState().loadAiStatus();
      } catch (e) {
        if (!cancelled) useAppStore.getState().setError(toErrorMessage(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  async function toggle() {
    const s = store();
    const next = !s.aiEnabled;
    s.setError(null);
    s.setAiEnabled(next);
    // Persisted state may differ (e.g. a failed write); trust the backend.
    await s.loadAiStatus();
  }

  async function runNow() {
    const s = store();
    s.setError(null);
    s.setDetectingFaces(true);
    s.setFacesProgress({ total: 0, done: 0, stage: "detecting faces", current: null });
    try {
      await api.startFaces();
    } catch (e) {
      s.setDetectingFaces(false);
      s.setFacesProgress(null);
      s.setError(toErrorMessage(e));
    }
  }

  async function stop() {
    try {
      await api.stopFaces();
    } catch (e) {
      store().setError(toErrorMessage(e));
    }
  }

  if (!aiStatus) {
    return (
      <div className="card" style={{ marginBottom: 16 }}>
        <h3>Local intelligence</h3>
        <div className="faint">Loading…</div>
      </div>
    );
  }

  const available = aiStatus.runtime_available;

  return (
    <div className="card" style={{ marginBottom: 16 }}>
      <h3>Local intelligence</h3>
      <p className="faint" style={{ marginTop: 0, marginBottom: 12 }}>
        Optional face detection, run entirely on this machine by a small local
        model. Everything else in PhotoGremlin works with it off.
      </p>

      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 16,
          padding: "10px 12px",
          border: "1px solid var(--border-subtle)",
          borderRadius: 8,
        }}
      >
        <div>
          <div style={{ fontSize: 14 }}>Detect faces in photographs</div>
          <div className="faint" style={{ fontSize: 12.5, marginTop: 2 }}>
            When on, new photographs are checked for faces automatically after
            each scan; you can also run it on demand below.
          </div>
        </div>
        <button
          className={`btn btn-sm ${aiEnabled ? "btn-primary" : ""}`}
          onClick={() => void toggle()}
          disabled={!available}
          title={available ? undefined : "Unavailable: the local ONNX Runtime is missing on this machine."}
        >
          {aiEnabled ? "On" : "Off"}
        </button>
      </div>

      <div style={{ height: 12 }} />

      <div className={`faint` + (available ? "" : " runtime-warn")} style={{ fontSize: 12.5 }}>
        {runtimeLine(aiStatus)}
      </div>

      {available && (
        <>
          <div className="faint" style={{ fontSize: 12.5, margin: "6px 0 0" }}>
            Model: {aiStatus.model} ({formatModelSize(aiStatus.model_bytes)} embedded,
            never downloaded).
          </div>
          <div className="faint" style={{ fontSize: 12.5, margin: "6px 0 0" }}>
            {formatFacesProgressLine(aiStatus)}
          </div>
          <div className="faint" style={{ fontSize: 12.5, margin: "6px 0 0" }}>
            Scene model: {aiStatus.scene_model} ({formatModelSize(aiStatus.scene_model_bytes)}{" "}
            embedded, never downloaded).
          </div>
          <div className="faint" style={{ fontSize: 12.5, margin: "6px 0 0" }}>
            {formatScenesProgressLine(aiStatus)}
          </div>

          <div style={{ display: "flex", gap: 8, marginTop: 12, alignItems: "center", flexWrap: "wrap" }}>
            {detectingFaces ? (
              <button className="btn btn-sm btn-danger" onClick={() => void stop()}>
                Stop detection
              </button>
            ) : (
              <button
                className="btn btn-sm"
                onClick={() => void runNow()}
                disabled={aiStatus.photo_count === 0}
                title={
                  aiStatus.photo_count === 0
                    ? "Add a photo folder first."
                    : "Run face detection over every photograph that still needs it (re-runs are incremental)."
                }
              >
                Detect faces now
              </button>
            )}
            {facesProgress && facesProgress.total > 0 && (
              <span className="faint" style={{ fontSize: 12.5 }}>
                {facesProgress.done.toLocaleString()} of {facesProgress.total.toLocaleString()}
                {facesProgress.current ? ` — ${facesProgress.current}` : ""}
              </span>
            )}
          </div>

          {facesSummary && !detectingFaces && (
            <div className="faint" style={{ fontSize: 12.5, marginTop: 10 }}>
              {formatFaceSummaryLine(facesSummary)}
            </div>
          )}
          {facesSummary && facesSummary.failed > 0 && facesSummary.errors.length > 0 && (
            <div
              role="alert"
              style={{ fontSize: 12, color: "var(--warning)", marginTop: 4 }}
            >
              Example: {facesSummary.errors[0]}
            </div>
          )}
        </>
      )}
      {error && (
        <div role="alert" style={{ fontSize: 12, color: "var(--danger)", marginTop: 10 }}>
          {error}
        </div>
      )}
    </div>
  );
}

export function SettingsView() {
  const appInfo = useAppStore((s) => s.appInfo);
  const paths = useAppStore((s) => s.paths);
  const theme = useAppStore((s) => s.theme);
  const setTheme = useAppStore((s) => s.setTheme);

  return (
    <div>
      <div className="card" style={{ marginBottom: 16 }}>
        <h3>Appearance</h3>
        <p className="faint" style={{ marginTop: 0, marginBottom: 12 }}>
          Choose neutral graphite for a darkroom workflow or clean silver for
          a light workspace. Both use the same quiet blue controls, and the
          choice stays on this machine.
        </p>
        <div className="segmented" role="radiogroup" aria-label="Theme">
          <button
            role="radio"
            aria-checked={theme === "dark"}
            className={theme === "dark" ? "is-on" : ""}
            onClick={() => setTheme("dark")}
          >
            <MoonIcon />
            Darkroom
          </button>
          <button
            role="radio"
            aria-checked={theme === "light"}
            className={theme === "light" ? "is-on" : ""}
            onClick={() => setTheme("light")}
          >
            <SunIcon />
            Gallery light
          </button>
        </div>
      </div>

      <div className="card" style={{ marginBottom: 16 }}>
        <h3>Application</h3>
        <div className="stat-grid">
          <div className="stat-card">
            <div className="label">Name</div>
            <div className="value" style={{ fontSize: 16 }}>
              {appInfo?.name ?? "PhotoGremlin"}
            </div>
          </div>
          <div className="stat-card">
            <div className="label">Version</div>
            <div className="value" style={{ fontSize: 16 }}>
              <span className="mono">{appInfo?.version ?? "dev"}</span>
            </div>
          </div>
          <div className="stat-card">
            <div className="label">Platform</div>
            <div className="value" style={{ fontSize: 16 }}>
              {appInfo?.platform ?? "—"}
            </div>
          </div>
        </div>
      </div>

      <LocalIntelligenceCard />

      <EditingApplicationCard />

      <StorageMaintenanceCard />

      <div className="card" style={{ marginBottom: 16 }}>
        <h3>Keyboard shortcuts</h3>
        <div style={{ display: "grid", gap: 6 }}>
          {SHORTCUTS.map((s) => (
            <div
              key={s.id}
              style={{ display: "flex", gap: 14, alignItems: "center", fontSize: 13 }}
            >
              <span className="mono" style={{ minWidth: 110, color: "var(--text-dim)" }}>
                {s.keys}
              </span>
              <span>{s.action}</span>
              {s.scope === "viewer" && (
                <span className="faint" style={{ fontSize: 11.5, marginLeft: "auto" }}>
                  in the viewer
                </span>
              )}
            </div>
          ))}
        </div>
      </div>

      <div className="card" style={{ marginBottom: 16 }}>
        <h3>Privacy contract</h3>
        <div style={{ display: "flex", gap: 10, alignItems: "flex-start" }}>
          <span style={{ color: "var(--accent)" }}>
            <ShieldIcon size={18} />
          </span>
          <div>
            <p style={{ marginBottom: 8 }}>{appInfo?.privacy}</p>
            <ul style={{ paddingLeft: 18, color: "var(--text-dim)", fontSize: 13, display: "grid", gap: 4 }}>
              <li>No account, login, or cloud dependency.</li>
              <li>Zero network requests at runtime — fully usable offline.</li>
              <li>All analysis (sharpness, exposure, similarity, statistics) runs on this machine.</li>
              <li>No telemetry, analytics, or tracking of any kind.</li>
              <li>Any optional visual intelligence uses small local models only.</li>
            </ul>
          </div>
        </div>
      </div>

      <div className="card" style={{ marginBottom: 16 }}>
        <h3>Projects</h3>
        <p className="faint" style={{ marginTop: 0, marginBottom: 10 }}>
          Recent projects are the last folders you opened — up to the cap below. Removing from the list never deletes photos or folders on disk.
        </p>
        <button
          className="btn btn-sm"
          onClick={async () => {
            try { await api.clearRecentProjects(); await useAppStore.getState().refreshRecentProjects(); } catch (e) { useAppStore.getState().setError(toErrorMessage(e)); }
          }}
        >
          Clear recent projects
        </button>
      </div>

      <div className="card">
        <h3>Local storage</h3>
        {paths ? (
          <>
            <PathRow label="Catalog database" value={paths.db_path} />
            <div style={{ height: 8 }} />
            <PathRow label="Data directory" value={paths.data_dir} />
            <div style={{ height: 8 }} />
            <PathRow label="Thumbnail cache" value={paths.thumbnails_dir} />
            <div style={{ height: 8 }} />
            <PathRow label="Log directory" value={paths.log_dir} />
          </>
        ) : (
          <div className="faint">Loading…</div>
        )}
      </div>
    </div>
  );
}
