import { useEffect, useState } from "react";
import { api, toErrorMessage } from "@/lib/ipc";
import { useAppStore } from "@/stores/appStore";
import { EmptyState } from "@/components/EmptyState";
import { chipLabel, filterToDraft } from "@/features/library/filterFields";
import { cleanName } from "@/features/organize/labels";
import type { Filter, SavedView } from "@/types/api";

function fmtDate(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
}

/**
 * Saved views (Sprint 8): named, dynamic filters. Applying a view loads its
 * conditions into the library filter and navigates there — the view stays
 * correct as the library changes because it is a filter, not a snapshot.
 */
export function SavedViewsView() {
  const activeFolder = useAppStore((s) => s.activeFolder);
  const savedViews = useAppStore((s) => s.savedViews);
  const store = useAppStore.getState;
  const [counts, setCounts] = useState<Record<number, number>>({});
  const [error, setError] = useState<string | null>(null);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editName, setEditName] = useState("");
  const [busy, setBusy] = useState(false);

  // Load views, then their live (dynamic) photograph counts.
  useEffect(() => {
    let cancelled = false;
    setCounts({});
    void (async () => {
      await store().loadSavedViews();
      const views = store().savedViews ?? [];
      const results = await Promise.all(
        views.map(async (v) => {
          try {
            return [v.id, await api.savedViewCount(v.id)] as const;
          } catch {
            return null;
          }
        }),
      );
      if (cancelled) return;
      const map: Record<number, number> = {};
      for (const r of results) if (r) map[r[0]] = r[1];
      setCounts(map);
    })();
    return () => {
      cancelled = true;
    };
  }, [activeFolder, store]);

  function open(view: SavedView) {
    try {
      const filter = JSON.parse(view.filter_json) as Filter;
      store().setFilterConditions(filterToDraft(filter));
      store().setView("library");
    } catch {
      store().setError("This saved view's filter could not be read.");
    }
  }

  async function startRename(view: SavedView) {
    setEditingId(view.id);
    setEditName(view.name);
  }

  async function commitRename(id: number) {
    const name = cleanName(editName);
    if (name === null) {
      setError("View name must be 1–60 characters.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await api.renameSavedView(id, name);
      setEditingId(null);
      await store().loadSavedViews();
    } catch (e) {
      setError(toErrorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function remove(id: number) {
    setError(null);
    try {
      await api.deleteSavedView(id);
      await store().loadSavedViews();
    } catch (e) {
      setError(toErrorMessage(e));
    }
  }

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

  const views = savedViews ?? [];
  if (!savedViews) {
    return <div className="faint" style={{ padding: 20 }}>Loading saved views…</div>;
  }

  if (views.length === 0) {
    return (
      <EmptyState glyph="≣" title="No saved views yet">
        <p>
          A saved view stores a filter — not a list of images. In the Library, add
          conditions to the filter bar (for example “sharpness ≥ 70 and ISO
          below 1600”) and press “Save view”. You can return to it here, and it
          keeps working as your library changes.
        </p>
      </EmptyState>
    );
  }

  return (
    <div>
      <div className="card">
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <h3>Saved views ({views.length})</h3>
          <span className="faint" style={{ fontSize: 12 }}>
            Counts are live — they follow the current library
          </span>
        </div>
        <table className="table" style={{ marginTop: 8 }}>
          <thead>
            <tr>
              <th>View</th>
              <th>Conditions</th>
              <th style={{ textAlign: "right" }}>Photographs</th>
              <th>Saved</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {views.map((v) => {
              let conditions: ReturnType<typeof filterToDraft> = [];
              try {
                conditions = filterToDraft(JSON.parse(v.filter_json) as Filter);
              } catch {
                conditions = [];
              }
              return (
                <tr key={v.id}>
                  <td style={{ fontWeight: 600 }}>
                    {editingId === v.id ? (
                      <input
                        className="input"
                        style={{ width: 220 }}
                        value={editName}
                        autoFocus
                        onChange={(e) => setEditName(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") void commitRename(v.id);
                          if (e.key === "Escape") setEditingId(null);
                        }}
                        aria-label="View name"
                      />
                    ) : (
                      <button
                        className="btn btn-sm btn-ghost"
                        style={{ padding: 0, fontSize: 13, textAlign: "left" }}
                        onClick={() => open(v)}
                        title="Open in the Library"
                      >
                        {v.name}
                      </button>
                    )}
                    {v.description && (
                      <div className="faint" style={{ fontSize: 11.5 }}>{v.description}</div>
                    )}
                  </td>
                  <td className="faint" style={{ fontSize: 12, maxWidth: 380 }}>
                    {conditions.length === 0
                      ? "all photographs"
                      : conditions.map((c, i) => (
                          <span key={i}>
                            {i > 0 && " · "}
                            {chipLabel(c)}
                          </span>
                        ))}
                  </td>
                  <td className="mono" style={{ textAlign: "right" }}>
                    {counts[v.id] !== undefined ? counts[v.id].toLocaleString() : "…"}
                  </td>
                  <td className="faint" style={{ fontSize: 12 }}>
                    {fmtDate(v.updated_at)}
                  </td>
                  <td style={{ textAlign: "right", whiteSpace: "nowrap" }}>
                    {editingId === v.id ? (
                      <>
                        <button className="btn btn-sm" disabled={busy} onClick={() => void commitRename(v.id)}>
                          Save
                        </button>{" "}
                        <button className="btn btn-ghost btn-sm" onClick={() => setEditingId(null)}>
                          Cancel
                        </button>
                      </>
                    ) : (
                      <>
                        <button className="btn btn-ghost btn-sm" onClick={() => open(v)}>
                          Open
                        </button>{" "}
                        <button className="btn btn-ghost btn-sm" onClick={() => void startRename(v)}>
                          Rename
                        </button>{" "}
                        <button className="btn btn-ghost btn-sm" onClick={() => void remove(v.id)}>
                          Delete
                        </button>
                      </>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}
