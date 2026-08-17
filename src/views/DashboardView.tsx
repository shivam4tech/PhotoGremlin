import { useEffect, useState } from "react";
import { useAppStore } from "@/stores/appStore";
import { api, toErrorMessage } from "@/lib/ipc";
import { periodJson, type PeriodStats, type UsageCount } from "@/types/api";
import { StatCard } from "@/components/StatCard";
import { EmptyState } from "@/components/EmptyState";
import { fmtIso, fmtMetric, fmtRatio, fmtShare, maxCount, monthLabel } from "@/features/stats/format";

const PERIODS = [
  ["today", "Today"],
  ["this-week", "This week"],
  ["this-month", "This month"],
  ["this-year", "This year"],
  ["all", "All time"],
] as const;

function Histogram({ title, bins }: { title: string; bins: { label: string; count: number }[] }) {
  const max = maxCount(bins);
  const any = max > 0;
  return (
    <div className="card">
      <h3>{title}</h3>
      {!any && (
        <div className="faint" style={{ fontSize: 12.5, marginTop: 6 }}>
          No data in this period.
        </div>
      )}
      <div style={{ display: "flex", flexDirection: "column", gap: 7, marginTop: any ? 12 : 0 }}>
        {bins.map((b) => (
          <div key={b.label} className="dist-row">
            <span className="dist-label mono">{b.label}</span>
            <div className="progress-track" role="presentation">
              <div
                className="progress-fill dist-fill"
                style={{ width: max > 0 ? `${(b.count / max) * 100}%` : "0%" }}
              />
            </div>
            <span className="dist-count mono">{b.count}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

function UsageTable({ title, rows }: { title: string; rows: UsageCount[] }) {
  if (rows.length === 0) {
    return (
      <div className="card">
        <h3>{title}</h3>
        <div className="faint" style={{ fontSize: 12.5, marginTop: 6 }}>
          No data in this period.
        </div>
      </div>
    );
  }
  return (
    <div className="card">
      <h3>{title}</h3>
      <table className="table" style={{ marginTop: 8 }}>
        <thead>
          <tr>
            <th>Name</th>
            <th style={{ textAlign: "right" }}>Photos</th>
            <th style={{ textAlign: "right" }}>Share</th>
            <th style={{ textAlign: "right" }}>Avg sharpness</th>
            <th style={{ textAlign: "right" }}>Avg ISO</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((u) => (
            <tr key={u.name}>
              <td>{u.name}</td>
              <td className="mono" style={{ textAlign: "right" }}>
                {u.photos}
              </td>
              <td className="mono" style={{ textAlign: "right" }}>
                {Math.round(u.share)}%
              </td>
              <td className="mono" style={{ textAlign: "right" }}>
                {fmtMetric(u.avg_sharpness)}
              </td>
              <td className="mono" style={{ textAlign: "right" }}>
                {fmtIso(u.avg_iso)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <div className="faint" style={{ fontSize: 11.5, marginTop: 10 }}>
        Usage as measured, not ranked. No verdicts on equipment.
      </div>
    </div>
  );
}

export function DashboardView() {
  const dbStatus = useAppStore((s) => s.dbStatus);
  const [kind, setKind] = useState<string>("all");
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [stats, setStats] = useState<PeriodStats | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setStats(null);
    setError(null);
    api
      .periodStats(periodJson(kind, from, to))
      .then((s) => {
        if (!cancelled) setStats(s);
      })
      .catch((e) => {
        if (!cancelled) setError(toErrorMessage(e));
      });
    return () => {
      cancelled = true;
    };
  }, [kind, from, to, dbStatus?.photo_count]);

  if (!dbStatus || dbStatus.photo_count === 0) {
    return (
      <EmptyState glyph="◔" title="No data yet">
        <p>
          The dashboard answers real questions — which sessions you shot, what lenses you
          actually reach for, how your sharpness and ISO usage trend over time.
          <br />
          <br />
          Import a photo folder and index it, and this becomes your photographer's
          workspace.
        </p>
      </EmptyState>
    );
  }

  const analyzedPct =
    dbStatus.photo_count > 0
      ? Math.round(((stats?.analyzed ?? 0) / dbStatus.photo_count) * 100)
      : 0;

  return (
    <div>
      <div style={{ display: "flex", gap: 8, marginBottom: 20, flexWrap: "wrap", alignItems: "center" }}>
        {PERIODS.map(([k, label]) => (
          <button
            key={k}
            className={`btn btn-sm${kind === k ? " btn-primary" : ""}`}
            onClick={() => setKind(k)}
          >
            {label}
          </button>
        ))}
        <button
          className={`btn btn-sm${kind === "custom" ? " btn-primary" : ""}`}
          onClick={() => setKind("custom")}
        >
          Range…
        </button>
        {kind === "custom" && (
          <span style={{ display: "inline-flex", gap: 6, alignItems: "center" }}>
            <input
              type="date"
              className="input"
              value={from}
              onChange={(e) => setFrom(e.target.value)}
              aria-label="From date"
            />
            <span className="faint">→</span>
            <input
              type="date"
              className="input"
              value={to}
              onChange={(e) => setTo(e.target.value)}
              aria-label="To date"
            />
          </span>
        )}
      </div>

      {error && (
        <div
          style={{
            padding: "10px 14px",
            borderRadius: 8,
            background: "var(--danger-soft)",
            color: "var(--danger)",
            fontSize: 12.5,
            marginBottom: 16,
          }}
        >
          {error}
        </div>
      )}

      {!stats ? (
        <div className="faint" style={{ padding: 20 }}>
          {kind === "custom" && (!from || !to)
            ? "Pick the range to compute statistics for."
            : "Computing statistics…"}
        </div>
      ) : (
        <>
          <div className="stat-grid" style={{ marginBottom: 4 }}>
            <StatCard label="Photographs" value={stats.photos} sub={stats.period} />
            <StatCard
              label="Sessions"
              value={stats.sessions}
              sub={
                stats.photos_per_session !== null
                  ? `${stats.photos_per_session.toFixed(1)} per session`
                  : "—"
              }
            />
            <StatCard
              label="Analyzed"
              value={stats.analyzed}
              sub={`${analyzedPct}% of this period`}
            />
          </div>

          <div className="section-title">Characteristics — averages of {stats.analyzed} analyzed</div>
          {stats.analyzed === 0 ? (
            <div className="card">
              <div className="faint" style={{ fontSize: 13 }}>
                No analyzed photographs in this period. Run analysis in the Library and
                these characteristics appear — nothing is estimated in the meantime.
              </div>
            </div>
          ) : (
            <div className="stat-grid">
              <StatCard label="Sharpness" value={fmtMetric(stats.avg_sharpness)} sub="0–100" />
              <StatCard label="Brightness" value={fmtMetric(stats.avg_brightness)} sub="0–100" />
              <StatCard label="Contrast" value={fmtMetric(stats.avg_contrast)} sub="0–100" />
              <StatCard label="Saturation" value={fmtMetric(stats.avg_saturation)} sub="0–100" />
            </div>
          )}

          <div className="section-title">Composition shares</div>
          <div className="stat-grid">
            <StatCard label="Monochrome" value={fmtShare(stats.monochrome_share)} sub="of analyzed" />
            <StatCard label="Color" value={fmtShare(stats.color_share)} sub="of analyzed" />
            {stats.faces_present_share !== null && (
              <StatCard label="Faces present" value={fmtShare(stats.faces_present_share)} sub="of photos with face data" />
            )}
            {stats.smiling_share !== null && (
              <StatCard label="Smiling" value={fmtShare(stats.smiling_share)} sub="of photos with smile data" />
            )}
          </div>

          <div className="section-title">Distributions</div>
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(auto-fit, minmax(260px, 1fr))",
              gap: 12,
            }}
          >
            <Histogram title="ISO" bins={stats.iso_histogram} />
            <Histogram title="Aperture" bins={stats.aperture_histogram} />
            <Histogram title="Focal length" bins={stats.focal_histogram} />
            <Histogram title="Shutter" bins={stats.shutter_histogram} />
          </div>

          <div className="section-title">Usage</div>
          <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(340px, 1fr))", gap: 12 }}>
            <UsageTable title="Cameras" rows={stats.camera_usage} />
            <UsageTable title="Lenses" rows={stats.lens_usage} />
          </div>

          <div className="section-title">Trend by month</div>
          {stats.trend.length === 0 ? (
            <div className="card">
              <div className="faint" style={{ fontSize: 13 }}>
                No months with photographs in this period — only real months are shown,
                never filled-in ones.
              </div>
            </div>
          ) : (
            <div className="card">
              <table className="table">
                <thead>
                  <tr>
                    <th>Month</th>
                    <th style={{ textAlign: "right" }}>Photos</th>
                    <th style={{ textAlign: "right" }}>Sessions</th>
                    <th style={{ textAlign: "right" }}>Avg sharpness</th>
                    <th style={{ textAlign: "right" }}>Avg ISO</th>
                    <th style={{ textAlign: "right" }}>Color share</th>
                  </tr>
                </thead>
                <tbody>
                  {stats.trend.map((t) => (
                    <tr key={t.month}>
                      <td>{monthLabel(t.month)}</td>
                      <td className="mono" style={{ textAlign: "right" }}>{t.photos}</td>
                      <td className="mono" style={{ textAlign: "right" }}>{t.sessions}</td>
                      <td className="mono" style={{ textAlign: "right" }}>{fmtMetric(t.avg_sharpness)}</td>
                      <td className="mono" style={{ textAlign: "right" }}>{fmtIso(t.avg_iso)}</td>
                      <td className="mono" style={{ textAlign: "right" }}>{fmtShare(t.color_share)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <div className="faint" style={{ fontSize: 11.5, marginTop: 10 }}>
                Averages use the analyzed photographs of each month. Months without
                data never appear.
              </div>
            </div>
          )}

          {stats.selection && (
            <>
              <div className="section-title">Selection</div>
              <div className="card">
                <div className="stat-grid">
                  <StatCard label="Imported" value={stats.selection.imported} sub="this period" />
                  <StatCard label="Selected" value={stats.selection.selected} sub="kept state" />
                  <StatCard label="Rejected" value={stats.selection.rejected} sub="culled state" />
                  <StatCard label="Trashed" value={stats.selection.trashed} sub="all time" />
                  <StatCard
                    label="Selected ÷ imported"
                    value={fmtRatio(stats.selection.kept_ratio)}
                    sub="only counts selection state"
                  />
                </div>
                <div className="faint" style={{ fontSize: 11.5, marginTop: 10 }}>
                  The ratio appears as soon as selection state or file operations exist;
                  it is a measured ratio, not a goal.
                </div>
              </div>
            </>
          )}
        </>
      )}
    </div>
  );
}
