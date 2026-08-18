/**
 * Bulk marks for the selection (Sprint 13): rate 1–5 (tap again to clear),
 * toggle the flag, or assign/clear a color label across all selected photos.
 * One round-trip per click via `update_marks` (only the clicked mark
 * changes on every selected photo — others are untouched).
 */
import { useState } from "react";
import { api, toErrorMessage } from "@/lib/ipc";
import { MARK_COLORS } from "./marks";

export function MarksPanel({
  photoIds,
  onApplied,
}: {
  photoIds: number[];
  onApplied: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [label, setLabel] = useState<{ rating: number | null; flag: boolean | null; color: string | null } | null>(null);

  const apply = async (rating: number | null, flag: boolean | null, color: string | null) => {
    if (busy) return;
    setBusy(true);
    setError(null);
    setLabel(null);
    try {
      await api.updateMarks(photoIds, rating, flag, color);
      setLabel({ rating, flag, color });
      onApplied();
    } catch (e) {
      setError(toErrorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const hide = label !== null && !busy;

  return (
    <div className="marks-panel">
      <span className="marks-panel-title">
        {photoIds.length.toLocaleString()} selected
      </span>
      <span className="marks-panel-group" title="Rate all selected">
        {[1, 2, 3, 4, 5].map((s) => (
          <button
            key={s}
            className={`marks-star${(hide && label!.rating === s) ? " is-on" : ""}`}
            title={`Rate ${s} star${s === 1 ? "" : "s"}`}
            aria-label={`Rate ${s} star${s === 1 ? "" : "s"}`}
            disabled={busy}
            onClick={() => apply(s, null, null)}
          >
            ★
          </button>
        ))}
        {hide && label!.rating !== null && (
          <button className="btn btn-ghost btn-sm" disabled={busy} onClick={() => apply(0, null, null)}>
            Clear rating
          </button>
        )}
      </span>
      <span className="marks-panel-group">
        <button
          className={`btn btn-sm${(hide && label!.flag === true) ? " btn-on" : " btn-ghost"}`}
          disabled={busy}
          onClick={() => apply(null, true, null)}
          title="Flag all selected"
        >
          ⚑ Flag
        </button>
        <button
          className="btn btn-sm btn-ghost"
          disabled={busy}
          onClick={() => apply(null, false, null)}
          title="Clear the flag on all selected"
        >
          Unflag
        </button>
      </span>
      <span className="marks-panel-group" title="Color-label all selected">
        {MARK_COLORS.map((c) => (
          <button
            key={c.name}
            className={`marks-color${(hide && label!.color === c.name) ? " is-on" : ""}`}
            title={`Label ${c.name}`}
            aria-label={`Label ${c.name}`}
            disabled={busy}
            style={{ background: c.hex }}
            onClick={() => apply(null, null, c.name)}
          />
        ))}
        {hide && label!.color !== null && (
          <button className="btn btn-ghost btn-sm" disabled={busy} onClick={() => apply(null, null, "")}>
            Clear label
          </button>
        )}
      </span>
      {error && <span className="marks-panel-err">{error}</span>}
    </div>
  );
}