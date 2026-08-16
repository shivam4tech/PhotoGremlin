import { useState } from "react";
import { useAppStore } from "@/stores/appStore";
import { StatCard } from "@/components/StatCard";
import { EmptyState } from "@/components/EmptyState";

const PERIODS = ["Today", "This week", "This month", "This year", "All time"] as const;

export function DashboardView() {
  const dbStatus = useAppStore((s) => s.dbStatus);
  const [period, setPeriod] = useState<string>("All time");

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

  return (
    <div>
      <div style={{ display: "flex", gap: 8, marginBottom: 20 }}>
        {PERIODS.map((p) => (
          <button
            key={p}
            className={`btn btn-sm${period === p ? " btn-primary" : ""}`}
            onClick={() => setPeriod(p)}
          >
            {p}
          </button>
        ))}
      </div>

      <div className="stat-grid" style={{ marginBottom: 20 }}>
        <StatCard label="Sessions" value={dbStatus.session_count} sub={period} />
        <StatCard label="Photographs" value={dbStatus.photo_count} sub={period} />
        <StatCard
          label="Analyzed"
          value={dbStatus.analyzed_count}
          sub={`${dbStatus.photo_count ? Math.round((dbStatus.analyzed_count / dbStatus.photo_count) * 100) : 0}% of library`}
        />
      </div>

      <div className="section-title">Coming online as the pipeline fills</div>
      <div className="card">
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "1fr 1fr",
            gap: 10,
            fontSize: 13,
            color: "var(--text-dim)",
          }}
        >
          {[
            "Sharpness, brightness, contrast and saturation trends over time",
            "Session comparison — side-by-side statistics for your shoots",
            "Camera and lens usage",
            "ISO, aperture and focal-length distributions",
            "Monochrome vs color share",
            "Selection ratio for culling sessions",
          ].map((t) => (
            <div key={t} style={{ display: "flex", gap: 8 }}>
              <span style={{ color: "var(--accent)" }}>●</span>
              <span>{t}</span>
            </div>
          ))}
        </div>
        <div className="faint" style={{ marginTop: 14, fontSize: 12 }}>
          Every statistic is computed locally from the analysis data already in your
          catalog. No numbers are fabricated; when data is missing, the section says so.
        </div>
      </div>
    </div>
  );
}
