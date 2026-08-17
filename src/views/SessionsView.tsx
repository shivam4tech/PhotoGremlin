import { useEffect, useState } from "react";
import { EmptyState } from "@/components/EmptyState";
import { api, toErrorMessage } from "@/lib/ipc";
import { useAppStore } from "@/stores/appStore";
import type { SessionRow } from "@/types/api";

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

export function SessionsView() {
  const activeFolder = useAppStore((s) => s.activeFolder);
  const [sessions, setSessions] = useState<SessionRow[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const rows = await api.listSessions();
        if (!cancelled) setSessions(rows);
      } catch (e) {
        if (!cancelled) setError(toErrorMessage(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [activeFolder, useAppStore.getState().dbStatus]);

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
    <div className="card">
      <h3>Sessions ({sessions.length})</h3>
      <table className="table">
        <thead>
          <tr>
            <th>Session</th>
            <th>Root folder</th>
            <th style={{ textAlign: "right" }}>Photos</th>
            <th>Shoot period</th>
            <th>Indexed</th>
          </tr>
        </thead>
        <tbody>
          {sessions.map((s) => (
            <tr key={s.id}>
              <td style={{ fontWeight: 600 }}>{s.name}</td>
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
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
