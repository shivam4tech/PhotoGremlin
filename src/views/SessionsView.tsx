import { useEffect, useState } from "react";
import { EmptyState } from "@/components/EmptyState";
import { api, toErrorMessage } from "@/lib/ipc";
import { useAppStore } from "@/stores/appStore";
import type { SessionMetrics, SessionRow, SessionSummary } from "@/types/api";
import {
  fmtAperture,
  fmtDuration,
  fmtIso,
  fmtMetric,
  fmtShare,
  fmtShutter,
  monthLabel,
} from "@/features/stats/format";

const COMPARE_MAX = 8;

function fmtDate(iso: string | null): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

interface CompareRow {
  label: string;
  values: (string | number)[];
}

function ComparisonTable({ rows }: { rows: SessionMetrics[] }) {
  const metricRows: CompareRow[] = [
    { label: "Photographs", values: rows.map((r) => r.photos) },
    { label: "Analyzed", values: rows.map((r) => r.analyzed) },
    { label: "Avg sharpness", values: rows.map((r) => fmtMetric(r.avg_sharpness)) },
    { label: "Avg brightness", values: rows.map((r) => fmtMetric(r.avg_brightness)) },
    { label: "Avg contrast", values: rows.map((r) => fmtMetric(r.avg_contrast)) },
    { label: "Avg saturation", values: rows.map((r) => fmtMetric(r.avg_saturation)) },
    { label: "Monochrome", values: rows.map((r) => fmtShare(r.monochrome_share)) },
    { label: "Color", values: rows.map((r) => fmtShare(r.color_share)) },
    { label: "Avg ISO", values: rows.map((r) => fmtIso(r.avg_iso)) },
    { label: "Avg aperture", values: rows.map((r) => fmtAperture(r.avg_aperture)) },
    { label: "Avg shutter", values: rows.map((r) => fmtShutter(r.avg_shutter)) },
    { label: "Start", values: rows.map((r) => fmtDate(r.start_time)) },
    { label: "End", values: rows.map((r) => fmtDate(r.end_time)) },
    { label: "Duration", values: rows.map((r) => fmtDuration(r.duration_days)) },
  ];
  return (
    <div className="card" style={{ overflowX: "auto" }}>
      <table className="table">
        <thead>
          <tr>
            <th style={{ minWidth: 130 }}>Metric</th>
            {rows.map((r) => (
              <th key={r.id} style={{ minWidth: 110 }}>
                {r.name}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {metricRows.map((row) => (
            <tr key={row.label}>
              <td className="faint" style={{ fontSize: 12 }}>
                {row.label}
              </td>
              {row.values.map((v, i) => (
                <td key={i} className="mono">
                  {v}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
      <div className="faint" style={{ fontSize: 11.5, marginTop: 10 }}>
        Side-by-side measurements of each session. No verdicts — the differences are
        yours to interpret.
      </div>
    </div>
  );
}

function SessionDetail({ summary }: { summary: SessionSummary }) {
  const s = summary.stats;
  return (
    <div className="card" style={{ marginTop: 12 }}>
      <h3>
        {summary.session.name}
        <span className="faint" style={{ fontWeight: 400, marginLeft: 10, fontSize: 12 }}>
          duration {fmtDuration(summary.duration_days)}
        </span>
      </h3>
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fit, minmax(130px, 1fr))",
          gap: 10,
          marginTop: 12,
        }}
      >
        {(
          [
            ["Photographs", String(s.photos)],
            ["Analyzed", String(s.analyzed)],
            ["Sharpness", fmtMetric(s.avg_sharpness)],
            ["Brightness", fmtMetric(s.avg_brightness)],
            ["Contrast", fmtMetric(s.avg_contrast)],
            ["Saturation", fmtMetric(s.avg_saturation)],
            ["Monochrome", fmtShare(s.monochrome_share)],
            ["Color", fmtShare(s.color_share)],
            ["Faces present", fmtShare(s.faces_present_share)],
            ["Smiling", fmtShare(s.smiling_share)],
          ] as [string, string][]
        ).map(([label, value]) => (
          <div key={label}>
            <div className="label">{label}</div>
            <div className="mono" style={{ fontSize: 15, fontWeight: 600 }}>
              {value}
            </div>
          </div>
        ))}
      </div>
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12, marginTop: 14 }}>
        {(
          [
            ["Top cameras", s.camera_usage.slice(0, 3)],
            ["Top lenses", s.lens_usage.slice(0, 3)],
          ] as [string, typeof s.camera_usage][]
        ).map(([title, rows]) => (
          <div key={title}>
            <div className="label">{title}</div>
            {rows.length === 0 ? (
              <div className="faint" style={{ fontSize: 12.5 }}>
                No data.
              </div>
            ) : (
              rows.map((u) => (
                <div key={u.name} style={{ fontSize: 12.5, padding: "3px 0" }}>
                  <span style={{ color: "var(--text-dim)" }}>{Math.round(u.share)}%</span>{" "}
                  {u.name}{" "}
                  <span className="faint mono">({u.photos} photos)</span>
                </div>
              ))
            )}
          </div>
        ))}
      </div>
      {s.trend.length > 0 && (
        <>
          <div className="label" style={{ marginTop: 14 }}>
            Months with photographs
          </div>
          <div style={{ fontSize: 12.5, color: "var(--text-dim)" }}>
            {s.trend
              .slice(-6)
              .map((t) => (
                <span key={t.month} style={{ marginRight: 12 }}>
                  {monthLabel(t.month)}: {t.photos} photos
                  {t.avg_sharpness !== null ? `, sharpness ${t.avg_sharpness.toFixed(1)}` : ""}
                </span>
              ))}
          </div>
        </>
      )}
    </div>
  );
}

export function SessionsView() {
  const activeFolder = useAppStore((s) => s.activeFolder);
  const [sessions, setSessions] = useState<SessionRow[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const [compareIds, setCompareIds] = useState<number[]>([]);
  const [comparison, setComparison] = useState<SessionMetrics[] | null>(null);
  const [compareRunning, setCompareRunning] = useState(false);

  const [detailId, setDetailId] = useState<number | null>(null);
  const [detail, setDetail] = useState<SessionSummary | null>(null);
  const [detailRunning, setDetailRunning] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setSessions(null);
    setCompareIds([]);
    setComparison(null);
    setDetail(null);
    api
      .listSessions()
      .then((rows) => {
        if (!cancelled) setSessions(rows);
      })
      .catch((e) => {
        if (!cancelled) setError(toErrorMessage(e));
      });
    return () => {
      cancelled = true;
    };
  }, [activeFolder, useAppStore.getState().dbStatus]);

  const toggleCompare = (id: number) => {
    setComparison(null);
    setDetail(null);
    setDetailId(null);
    setCompareIds((prev) =>
      prev.includes(id)
        ? prev.filter((x) => x !== id)
        : prev.length >= COMPARE_MAX
          ? prev
          : [...prev, id],
    );
  };

  const runCompare = async () => {
    setCompareRunning(true);
    setError(null);
    try {
      setComparison(await api.compareSessions(compareIds));
    } catch (e) {
      setError(toErrorMessage(e));
    } finally {
      setCompareRunning(false);
    }
  };

  /** Apply a `session_id = N` filter and open the Library on that session. */
  const openInLibrary = (id: number) => {
    useAppStore
      .getState()
      .setFilterConditions([{ field: "session_id", operator: "=", value: id }]);
    useAppStore.getState().setView("library");
  };

  const toggleDetail = async (id: number) => {
    if (detailId === id) {
      setDetailId(null);
      setDetail(null);
      return;
    }
    setDetailId(id);
    setDetailRunning(true);
    try {
      setDetail(await api.sessionSummary(id));
    } catch (e) {
      setError(toErrorMessage(e));
      setDetail(null);
    } finally {
      setDetailRunning(false);
    }
  };

  if (error) {
    return (
      <div
        style={{
          padding: "10px 14px",
          borderRadius: 8,
          background: "var(--danger-soft)",
          color: "var(--danger)",
          fontSize: 12.5,
        }}
      >
        {error}
      </div>
    );
  }

  if (sessions && sessions.length === 0) {
    return (
      <EmptyState glyph="◱" title="No sessions yet">
        <p>
          A session is a shoot or an imported body of work. Scan a photo folder in
          the Library and a session is created for it. Then sessions become
          comparable: sharpness, ISO, focal lengths, and more, side by side.
        </p>
      </EmptyState>
    );
  }

  if (!sessions) {
    return <div className="faint" style={{ padding: 20 }}>Loading sessions…</div>;
  }

  return (
    <div>
      <div className="card">
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <h3>Sessions ({sessions.length})</h3>
          <span className="faint" style={{ fontSize: 12 }}>
            Tick 2–{COMPARE_MAX} sessions to compare · open a name for its detail
          </span>
        </div>
        <table className="table" style={{ marginTop: 8 }}>
          <thead>
            <tr>
              <th style={{ width: 34 }} />
              <th>Session</th>
              <th>Root folder</th>
              <th style={{ textAlign: "right" }}>Photos</th>
              <th>Shoot period</th>
              <th>Indexed</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {sessions.map((s) => (
              <tr key={s.id}>
                <td>
                  <input
                    type="checkbox"
                    checked={compareIds.includes(s.id)}
                    onChange={() => toggleCompare(s.id)}
                    aria-label={`Compare ${s.name}`}
                  />
                </td>
                <td style={{ fontWeight: 600 }}>
                  <button
                    className="btn btn-sm btn-ghost"
                    style={{ padding: 0, fontSize: 13 }}
                    onClick={() => toggleDetail(s.id)}
                  >
                    {s.name}
                  </button>
                </td>
                <td className="mono faint" style={{ fontSize: 11.5, wordBreak: "break-all" }}>
                  {s.root_path ?? "manual"}
                </td>
                <td className="mono" style={{ textAlign: "right" }}>
                  {s.photo_count.toLocaleString()}
                </td>
                <td className="faint" style={{ fontSize: 12 }}>
                  {s.start_time && s.end_time
                    ? `${fmtDate(s.start_time)} → ${fmtDate(s.end_time)}`
                    : "pending EXIF pass"}
                </td>
                <td className="faint" style={{ fontSize: 12 }}>
                  {fmtDate(s.created_at)}
                </td>
                <td style={{ textAlign: "right", whiteSpace: "nowrap" }}>
                  <button
                    className="btn btn-ghost btn-sm"
                    onClick={() => openInLibrary(s.id)}
                    title="Show this session's photographs in the Library"
                  >
                    Open in library
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {detailRunning && (
        <div className="faint" style={{ padding: "14px 4px" }}>
          Loading session statistics…
        </div>
      )}
      {detail && detailId !== null && <SessionDetail summary={detail} />}

      {compareIds.length >= 2 && (
        <div style={{ marginTop: 14, display: "flex", gap: 10, alignItems: "center" }}>
          <button
            className="btn btn-primary btn-sm"
            disabled={compareRunning}
            onClick={runCompare}
          >
            {compareRunning
              ? "Comparing…"
              : comparison
                ? "Re-compare"
                : `Compare ${compareIds.length} sessions`}
          </button>
          <button className="btn btn-sm" onClick={() => setCompareIds([])}>
            Clear selection
          </button>
        </div>
      )}
      {comparison && <div style={{ marginTop: 12 }}><ComparisonTable rows={comparison} /></div>}
    </div>
  );
}
