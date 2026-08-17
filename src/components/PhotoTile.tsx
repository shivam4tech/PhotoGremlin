import { useEffect, useState } from "react";
import { api, toErrorMessage } from "@/lib/ipc";
import type { PhotoSummary } from "@/types/api";

/**
 * Formats for which the Rust core deliberately does not generate previews
 * in v0.1 (no local pixel provider): RAW sensors + HEIC. The tile shows a
 * labelled placeholder instead of an error — the file is still indexed and
 * fully usable for everything else.
 */
const NO_PREVIEW_EXTENSIONS = new Set([
  "cr2", "cr3", "nef", "arw", "raf", "dng", "orf", "rw2",
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
}: {
  photo: PhotoSummary;
  onOpen: (id: number) => void;
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

  return (
    <button
      className={`tile${state.kind === "placeholder" || state.kind === "error" ? " tile-muted" : ""}`}
      onClick={() => onOpen(photo.id)}
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
    </button>
  );
}
