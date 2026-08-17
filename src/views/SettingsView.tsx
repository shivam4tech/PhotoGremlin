import { useEffect } from "react";
import { api, toErrorMessage } from "@/lib/ipc";
import { useAppStore } from "@/stores/appStore";
import { ShieldIcon } from "@/components/Icons";
import {
  formatFaceSummaryLine,
  formatFacesProgressLine,
  formatModelSize,
  runtimeLine,
} from "@/features/settings/ai";
import { SHORTCUTS } from "@/features/shortcuts";

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

  return (
    <div>
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
