import { useAppStore } from "@/stores/appStore";
import { ShieldIcon } from "@/components/Icons";

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
