import { useEffect, useState } from "react";
import { api, toErrorMessage } from "@/lib/ipc";
import { LABEL_HEX } from "@/features/library/marks";
import type { PhotoSummary } from "@/types/api";
import type { SelectionState } from "@/stores/appStore";

/**
 * Formats for which the Rust core deliberately does not generate previews.
 * RAW files are handled locally by embedded/paired previews and a bounded
 * decode fallback; HEIC remains indexed with a labelled placeholder.
 */
const NO_PREVIEW_EXTENSIONS = new Set([
  "heic", "heif",
]);

type TileState =
  | { kind: "loading" }
  | { kind: "ok"; url: string; width: number; height: number }
  | { kind: "placeholder"; msg: string }
  | { kind: "error"; msg: string };

export function isPreviewable(extension: string): boolean {
  return !NO_PREVIEW_EXTENSIONS.has(extension.toLowerCase());
}

/**
 * One grid tile. Request exactly one small thumbnail (grid size) from the
 * Rust core; cached thumbnails return instant, fresh ones are generated
 * under a bounded concurrency. Never loads full-resolution images — that
 * rule is what makes 50k-photo libraries scroll smoothly.
 */
export function PhotoTile({
  photo,
  onOpen,
  selectionMode = false,
  selection = null,
  onKeep,
  onReject,
  onClear,
  onTrash,
  onDeletePermanently,
  marksMode = "contextual",
}: {
  photo: PhotoSummary;
  onOpen: (id: number) => void;
  /** Culling mode (Sprint 7): show keep/reject controls on the tile. */
  selectionMode?: boolean;
  /** Current culling state of this photo, if any. */
  selection?: SelectionState | null;
  onKeep?: (id: number) => void;
  onReject?: (id: number) => void;
  onClear?: (id: number) => void;
  onTrash?: (id: number) => void;
  onDeletePermanently?: (id: number) => void;
  marksMode?: "contextual" | "always";
}) {
  const [state, setState] = useState<TileState>({ kind: "loading" });

  useEffect(() => {
    let cancelled = false;
    setState({ kind: "loading" });
    if (!isPreviewable(photo.extension)) {
      setState({ kind: "placeholder", msg: `${photo.extension.toUpperCase()} — no local preview in this version` });
      return;
    }
    api
      .getThumbnail(photo.id, "grid")
      .then((t) => {
        if (!cancelled) setState({ kind: "ok", url: t.data_url, width: t.width, height: t.height });
      })
      .catch((e) => {
        if (!cancelled) setState({ kind: "error", msg: toErrorMessage(e) });
      });
    return () => {
      cancelled = true;
    };
  }, [photo.id, photo.extension]);

  const dims = photo.width && photo.height ? `${photo.width}×${photo.height}` : photo.extension.toUpperCase();

  const tileClass =
    `tile` +
    (state.kind === "placeholder" || state.kind === "error" ? " tile-muted" : "") +
    (selection === "selected" ? " tile-kept" : "") +
    (selection === "rejected" ? " tile-rejected" : "");

  return (
    <div className={`${tileClass} tile-marks-${marksMode}`}>
      <button
        className="tile-open"
        onClick={() => {
          if (!selectionMode) onOpen(photo.id);
        }}
        disabled={selectionMode}
        title={photo.filename}
      >
        {state.kind === "loading" && <div className="tile-skeleton" aria-label="Generating preview" />}
        {state.kind === "ok" && (
          <img className="tile-img" src={state.url} alt={photo.filename} width={state.width} height={state.height} />
        )}
        {state.kind === "placeholder" && <div className="tile-ph">{state.msg}</div>}
        {state.kind === "error" && <div className="tile-ph tile-ph-err">{state.msg}</div>}
        <span className="tile-label">
          <span className="tile-name" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
            {photo.filename}
          </span>
          <span className="tile-dim mono">{dims}</span>
        </span>
        {(photo.rating ?? 0) > 0 && (
          <span className="tile-badge tile-badge-stars" title={`Rating: ${photo.rating}/5`}>
            {"★".repeat(photo.rating ?? 0)}
          </span>
        )}
        {photo.flag && (
          <span className="tile-badge tile-badge-flag" title="Flagged">
            ⚑
          </span>
        )}
        {photo.color_label && (
          <span
            className="tile-badge tile-badge-label"
            title={`Color label: ${photo.color_label}`}
            style={{ background: LABEL_HEX[photo.color_label] ?? "#8a8f98" }}
          />
        )}
      </button>
      {selectionMode && (
        <span className="tile-select">
          <button
            className={`tile-sel-btn${selection === "selected" ? " is-on" : ""}`}
            title="Keep (select)"
            aria-label="Keep"
            onClick={() => (selection === "selected" ? onClear?.(photo.id) : onKeep?.(photo.id))}
          >
            ✓
          </button>
          <button
            className={`tile-sel-btn tile-sel-reject${selection === "rejected" ? " is-on" : ""}`}
            title="Reject"
            aria-label="Reject"
            onClick={() => (selection === "rejected" ? onClear?.(photo.id) : onReject?.(photo.id))}
          >
            ✕
          </button>
        </span>
      )}
      {!selectionMode && (onTrash || onDeletePermanently) && (
        <details className="tile-file-menu">
          <summary title={`File actions for ${photo.filename}`} aria-label={`File actions for ${photo.filename}`}>
            •••
          </summary>
          <div className="tile-file-menu-popover">
            {onTrash && (
              <button
                onClick={(event) => {
                  event.currentTarget.closest("details")?.removeAttribute("open");
                  onTrash(photo.id);
                }}
              >
                Move to trash…
              </button>
            )}
            {onDeletePermanently && (
              <button
                className="is-danger"
                onClick={(event) => {
                  event.currentTarget.closest("details")?.removeAttribute("open");
                  onDeletePermanently(photo.id);
                }}
              >
                Delete permanently…
              </button>
            )}
          </div>
        </details>
      )}
    </div>
  );
}
