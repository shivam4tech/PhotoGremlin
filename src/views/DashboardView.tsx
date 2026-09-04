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

const DASHBOARD_SECTIONS = ["top", "characteristics", "composition", "distributions", "usage", "trend", "selection"] as const;
type SectionId = (typeof DASHBOARD_SECTIONS)[number];
const _SECTION_LABELS: Record<SectionId, string> = {
  top: "Top stats",
  characteristics: "Characteristics",
  composition: "Composition shares",
  distributions: "Distributions",
  usage: "Usage",
  trend: "Trend by month",
  selection: "Selection",
};
void _SECTION_LABELS;

function DashboardSections({
  stats, analyzedPct, hidden, sectionOrder, organizeMode, onToggle, onMove,
}: {
  stats: PeriodStats;
  analyzedPct: number;
  hidden: Set<string>;
  sectionOrder: SectionId[];
  organizeMode: boolean;
  onToggle: (id: SectionId) => void;
  onMove: (id: SectionId, dir: -1 | 1) => void;
}) {
  const blocks: Record<SectionId, React.ReactNode> = {
    top: (
      <div className="stat-grid" style={{ marginBottom: 4 }}>
        <StatCard label="Photographs" value={stats.photos} sub={stats.period} />
        <StatCard label="Sessions" value={stats.sessions} sub={stats.photos_per_session !== null ? `${stats.photos_per_session.toFixed(1)} per session` : "—"} />
        <StatCard label="Analyzed" value={stats.analyzed} sub={`${analyzedPct}% of this period`} />
      </div>
    ),
    characteristics: (
      <>
        <div className="section-title">Characteristics — averages of {stats.analyzed} analyzed</div>
        {stats.analyzed === 0 ? (
          <div className="card"><div className="faint" style={{ fontSize: 13 }}>No analyzed photographs in this period. Run analysis in the Library.</div></div>
        ) : (
          <div className="stat-grid">
            <StatCard label="Sharpness" value={fmtMetric(stats.avg_sharpness)} sub="0–100" />
            <StatCard label="Brightness" value={fmtMetric(stats.avg_brightness)} sub="0–100" />
            <StatCard label="Contrast" value={fmtMetric(stats.avg_contrast)} sub="0–100" />
            <StatCard label="Saturation" value={fmtMetric(stats.avg_saturation)} sub="0–100" />
          </div>
        )}
      </>
    ),
    composition: (
      <>
        <div className="section-title">Composition shares</div>
        <div className="stat-grid">
          <StatCard label="Monochrome" value={fmtShare(stats.monochrome_share)} sub="of analyzed" />
          <StatCard label="Color" value={fmtShare(stats.color_share)} sub="of analyzed" />
          {stats.faces_present_share !== null && <StatCard label="Faces present" value={fmtShare(stats.faces_present_share)} sub="of photos with face data" />}
          {stats.smiling_share !== null && <StatCard label="Smiling" value={fmtShare(stats.smiling_share)} sub="of photos with smile data" />}
        </div>
      </>
    ),
    distributions: (
      <>
        <div className="section-title">Distributions</div>
        <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(260px, 1fr))", gap: 12 }}>
          <Histogram title="ISO" bins={stats.iso_histogram} />
          <Histogram title="Aperture" bins={stats.aperture_histogram} />
          <Histogram title="Focal length" bins={stats.focal_histogram} />
          <Histogram title="Shutter" bins={stats.shutter_histogram} />
        </div>
      </>
    ),
    usage: (
      <>
        <div className="section-title">Usage</div>
        <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(340px, 1fr))", gap: 12 }}>
          <UsageTable title="Cameras" rows={stats.camera_usage} />
          <UsageTable title="Lenses" rows={stats.lens_usage} />
        </div>
      </>
    ),
    trend: stats.trend.length === 0 ? (
      <>
        <div className="section-title">Trend by month</div>
        <div className="card"><div className="faint" style={{ fontSize: 13 }}>No months with photographs in this period — only real months are shown.</div></div>
      </>
    ) : (
      <>
        <div className="section-title">Trend by month</div>
        <div className="card">
          <table className="table">
            <thead><tr><th>Month</th><th style={{ textAlign: "right" }}>Photos</th><th style={{ textAlign: "right" }}>Sessions</th><th style={{ textAlign: "right" }}>Avg sharpness</th><th style={{ textAlign: "right" }}>Avg ISO</th><th style={{ textAlign: "right" }}>Color share</th></tr></thead>
            <tbody>{stats.trend.map((t) => (<tr key={t.month}><td>{monthLabel(t.month)}</td><td className="mono" style={{ textAlign: "right" }}>{t.photos}</td><td className="mono" style={{ textAlign: "right" }}>{t.sessions}</td><td className="mono" style={{ textAlign: "right" }}>{fmtMetric(t.avg_sharpness)}</td><td className="mono" style={{ textAlign: "right" }}>{fmtIso(t.avg_iso)}</td><td className="mono" style={{ textAlign: "right" }}>{fmtShare(t.color_share)}</td></tr>))}</tbody>
          </table>
          <div className="faint" style={{ fontSize: 11.5, marginTop: 10 }}>Averages use the analyzed photographs of each month. Months without data never appear.</div>
        </div>
      </>
    ),
    selection: stats.selection ? (
      <>
        <div className="section-title">Selection</div>
        <div className="card">
          <div className="stat-grid">
            <StatCard label="Imported" value={stats.selection.imported} sub="this period" />
            <StatCard label="Selected" value={stats.selection.selected} sub="kept state" />
            <StatCard label="Rejected" value={stats.selection.rejected} sub="culled state" />
            <StatCard label="Trashed" value={stats.selection.trashed} sub="all time" />
            <StatCard label="Selected ÷ imported" value={fmtRatio(stats.selection.kept_ratio)} sub="only counts selection state" />
          </div>
          <div className="faint" style={{ fontSize: 11.5, marginTop: 10 }}>Measured ratio, not a goal.</div>
        </div>
      </>
    ) : null,
  };

  return (
    <>
      {sectionOrder.map((id) => {
        const content = blocks[id];
        if (!content) return null;
        const isHidden = hidden.has(id);
        if (isHidden && !organizeMode) return null;
        return (
          <div key={id} style={{ opacity: isHidden ? 0.4 : 1 }}>
            {organizeMode && (
              <div style={{ display: "flex", gap: 4, marginBottom: 4, alignItems: "center" }}>
                <span className="faint mono" style={{ fontSize: 11 }}>{_SECTION_LABELS[id]}</span>
                <span style={{ flex: 1 }} />
                <button className="btn btn-sm" aria-label={isHidden ? `Show ${id}` : `Hide ${id}`} onClick={() => onToggle(id)}>{isHidden ? "Show" : "Hide"}</button>
                <button className="btn btn-sm" aria-label={`Move ${id} up`} disabled={sectionOrder.indexOf(id) <= 0} onClick={() => onMove(id, -1)}>↑</button>
                <button className="btn btn-sm" aria-label={`Move ${id} down`} disabled={sectionOrder.indexOf(id) >= sectionOrder.length - 1} onClick={() => onMove(id, 1)}>↓</button>
              </div>
            )}
            {content}
          </div>
        );
      })}
    </>
  );
}

export function DashboardView() {
  const dbStatus = useAppStore((s) => s.dbStatus);
  const dashboardLayout = useAppStore((s) => s.dashboardLayout);
  const setDashboardLayout = useAppStore((s) => s.setDashboardLayout);
  const [organizeMode, setOrganizeMode] = useState(false);
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

  const sectionOrder: SectionId[] = dashboardLayout?.order.length
    ? (dashboardLayout.order.filter((id) => (DASHBOARD_SECTIONS as readonly string[]).includes(id)) as SectionId[]).concat(
        DASHBOARD_SECTIONS.filter((id) => !dashboardLayout.order.includes(id)),
      )
    : ([...DASHBOARD_SECTIONS] as SectionId[]);
  const hidden = new Set(dashboardLayout?.hidden ?? []);
  function toggleHidden(id: SectionId) {
    const next = new Set(hidden);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    void setDashboardLayout({ hidden: [...next], order: sectionOrder });
  }
  function moveSection(id: SectionId, dir: -1 | 1) {
    const idx = sectionOrder.indexOf(id);
    const j = idx + dir;
    if (j < 0 || j >= sectionOrder.length) return;
    const next = [...sectionOrder];
    [next[idx], next[j]] = [next[j], next[idx]];
    void setDashboardLayout({ hidden: [...hidden], order: next });
  }
  // wired in Sprint 20's section rendering; keep typecheck clean
  void toggleHidden; void moveSection; void hidden;

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
        <span style={{ flex: 1 }} />
        <button
          className={`btn btn-sm${organizeMode ? " btn-primary" : ""}`}
          onClick={() => setOrganizeMode((v) => !v)}
          aria-pressed={organizeMode}
          aria-label="Organize dashboard"
          title="Show, hide and reorder dashboard sections"
        >
          {organizeMode ? "Done" : "Organize"}
        </button>
      </div>
      {organizeMode && (
        <div className="faint" style={{ fontSize: 12, marginBottom: 14 }}>
          Hide sections with the eye, reorder with ↑↓. Changes save automatically.
        </div>
      )}

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
        <DashboardSections
          stats={stats}
          analyzedPct={analyzedPct}
          hidden={hidden}
          sectionOrder={sectionOrder}
          organizeMode={organizeMode}
          onToggle={toggleHidden}
          onMove={moveSection}
        />
      )}
    </div>
  );
}
