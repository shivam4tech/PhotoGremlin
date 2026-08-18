import { useEffect, useState } from "react";
import { api, toErrorMessage } from "@/lib/ipc";
import { MARK_COLORS } from "@/features/library/marks";
import type { PhotoFull, PhotoSummary } from "@/types/api";

type ImageState =
  | { kind: "loading" }
  | { kind: "ok"; url: string }
  | { kind: "placeholder"; msg: string }
  | { kind: "error"; msg: string };

function fmtBytes(n: number | null): string {
  if (n == null) return "—";
  if (n >= 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  if (n >= 1024) return `${(n / 1024).toFixed(0)} KB`;
  return `${n} B`;
}

/** Camera app convention: 1/250s above 1s, otherwise decimal seconds. */
function fmtShutter(seconds: number | null): string {
  if (seconds == null) return "—";
  if (seconds >= 1) return `${seconds.toFixed(2)}s`;
  if (seconds <= 0) return "—";
  const denom = Math.round(1 / seconds);
  if (denom >= 10) return `1/${denom}s`;
  return `${seconds.toFixed(3)}s`;
}

function fmtFocal(mm: number | null): string {
  return mm == null ? "—" : `${Math.round(mm)} mm`;
}

function fmtAperture(v: number | null): string {
  return v == null ? "—" : `f/${v.toFixed(1)}`;
}

function fmtDate(iso: string | null): string {
  if (!iso) return "—";
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? "—" : d.toLocaleString();
}

function Metric({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="meta-row">
      <span className="meta-label">{label}</span>
      <span className="meta-value mono">{value}</span>
    </div>
  );
}

function Section({ title, children, empty }: { title: string; children: React.ReactNode; empty?: boolean }) {
  return (
    <section className="viewer-section">
      <h4>{title}</h4>
      {empty ? <div className="meta-empty">No data recorded.</div> : children}
    </section>
  );
}

/**
 * Full-screen photo viewer (Sprint 3 "basic" tier).
 *
 * - Large image: a viewer-size thumbnail (≤1600 px on the long side… here
 *   the width cap) generated locally — the full-resolution file is never
 *   pulled into the webview, which keeps navigation instant.
 * - Metadata panel: file / capture / camera facts + the analysis slot
 *   (populated once the Sprint 4 analysis pass has run).
 * - Keyboard: Esc closes, ← / → move within the loaded page (a 96-tile
 *   window). Widen the window across pages in a later sprint if needed.
 */
export function Viewer({
  photoId,
  ordered,
  onClose,
  onNavigate,
}: {
  photoId: number;
  ordered: PhotoSummary[];
  onClose: () => void;
  onNavigate: (id: number) => void;
}) {
  const [full, setFull] = useState<PhotoFull | null>(null);
  const [image, setImage] = useState<ImageState>({ kind: "loading" });
  const [metaError, setMetaError] = useState<string | null>(null);

  // Apply a mark change (Sprint 13): only the given fields change; then
  // refresh the panel so the UI reflects the new row.
  const applyMark = (rating: number | null, flag: boolean | null, color: string | null) => {
    void api.updateMarks([photoId], rating, flag, color).then(() => {
      api.getPhotoFull(photoId).then(setFull).catch(() => {});
    });
  };

  useEffect(() => {
    let cancelled = false;
    setFull(null);
    setImage({ kind: "loading" });
    setMetaError(null);
    api.getPhotoFull(photoId).then((f) => {
      if (!cancelled) setFull(f);
    }).catch((e) => {
      if (!cancelled) setMetaError(toErrorMessage(e));
    });
    api.getThumbnail(photoId, "viewer").then((t) => {
      if (!cancelled) setImage({ kind: "ok", url: t.data_url });
    }).catch((e) => {
      if (!cancelled)
        setImage(toErrorMessage(e).toLowerCase().includes("supported")
          ? { kind: "placeholder", msg: "No local preview is available for this format yet." }
          : { kind: "error", msg: toErrorMessage(e) });
    });
    return () => {
      cancelled = true;
    };
  }, [photoId]);

  const idx = ordered.findIndex((p) => p.id === photoId);
  const hasPrev = idx > 0;
  const hasNext = idx >= 0 && idx < ordered.length - 1;

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      } else if (e.key === "ArrowLeft" && hasPrev) {
        e.preventDefault();
        onNavigate(ordered[idx - 1].id);
      } else if (e.key === "ArrowRight" && hasNext) {
        e.preventDefault();
        onNavigate(ordered[idx + 1].id);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [idx, hasPrev, hasNext, ordered, onClose, onNavigate]);

  const analyzed = full != null && (full.sharpness != null || full.brightness != null);

  return (
    <div className="viewer-backdrop" onClick={onClose}>
      <div className="viewer" onClick={(e) => e.stopPropagation()}>
        <div className="viewer-stage">
          {image.kind === "loading" && <div className="viewer-loader" />}
          {image.kind === "ok" && <img className="viewer-img" src={image.url} alt={full?.filename ?? "photograph"} />}
          {image.kind === "placeholder" && <div className="viewer-empty">{image.msg}</div>}
          {image.kind === "error" && <div className="viewer-empty viewer-empty-err">{image.msg}</div>}

          <div className="viewer-nav">
            <button className="btn btn-ghost" disabled={!hasPrev} onClick={() => onNavigate(ordered[idx - 1].id)} aria-label="Previous">←</button>
            <span className="faint mono" style={{ fontSize: 11.5 }}>
              {idx >= 0 ? `${idx + 1} / ${ordered.length}` : "—"}
            </span>
            <button className="btn btn-ghost" disabled={!hasNext} onClick={() => onNavigate(ordered[idx + 1].id)} aria-label="Next">→</button>
          </div>
        </div>

        <aside className="viewer-panel">
          <div className="viewer-panel-head">
            <div className="viewer-filename" title={full?.filename ?? ""}>
              {full?.filename ?? `Photograph #${photoId}`}
            </div>
            <button className="btn btn-ghost btn-sm" onClick={onClose} aria-label="Close viewer">Esc ✕</button>
          </div>

          {metaError && (
            <div style={{ margin: "0 0 12px", padding: "8px 10px", borderRadius: 6, background: "var(--danger-soft)", color: "var(--danger)", fontSize: 12 }}>
              {metaError}
            </div>
          )}

          <Section title="File">
            <Metric label="Dimensions" value={full ? (full.width && full.height ? `${full.width} × ${full.height}` : "—") : "…"} />
            <Metric label="Size" value={full ? fmtBytes(full.size_bytes) : "…"} />
            <Metric label="Format" value={full?.extension.toUpperCase() ?? "…"} />
            {full?.orientation && <Metric label="Orientation" value={full.orientation} />}
            {full && <Metric label="Geotagged" value={full.gps_present ? "GPS present" : "Not present"} />}
          </Section>

          <Section title="Capture">
            <Metric label="Taken" value={fmtDate(full?.capture_datetime ?? null)} />
            {full?.capture_datetime && full.capture_datetime_source && (
              <Metric
                label="Date source"
                value={
                  full.capture_datetime_source === "exif"
                    ? "EXIF"
                    : full.capture_datetime_source === "filename"
                      ? "Filename (estimated)"
                      : "File modified (estimated)"
                }
              />
            )}
          </Section>

          <Section title="Marks">
            {!full ? (
              <div className="meta-empty">Loading…</div>
            ) : (
              <>
                <div className="marks-row">
                  <span className="meta-label">Rating</span>
                  <span className="marks-stars" aria-label="Rating">
                    {[1, 2, 3, 4, 5].map((s) => (
                      <button
                        key={s}
                        className={`marks-star${(full.rating ?? 0) >= s ? " is-on" : ""}`}
                        title={`${s} ${s === 1 ? "star" : "stars"}`}
                        aria-pressed={(full.rating ?? 0) === s}
                        onClick={() =>
                          applyMark(
                            (full.rating ?? 0) === s ? 0 : s,
                            null,
                            null,
                          )
                        }
                      >
                        ★
                      </button>
                    ))}
                  </span>
                </div>
                <div className="marks-row">
                  <span className="meta-label">Flag</span>
                  <button
                    className={`btn btn-sm${full.flag ? " btn-on" : " btn-ghost"}`}
                    onClick={() => applyMark(null, !full.flag, null)}
                  >
                    {full.flag ? "⚑ Flagged" : "Flag"}
                  </button>
                </div>
                <div className="marks-row">
                  <span className="meta-label">Label</span>
                  <span className="marks-colors">
                    {MARK_COLORS.map((c) => (
                      <button
                        key={c.name}
                        className={`marks-color${full.color_label === c.name ? " is-on" : ""}`}
                        title={c.name}
                        aria-label={c.name}
                        style={{ background: c.hex }}
                        onClick={() =>
                          applyMark(null, null, full.color_label === c.name ? "" : c.name)
                        }
                      />
                    ))}
                  </span>
                </div>
              </>
            )}
          </Section>

          <Section title="Camera" empty={!full || (!full.camera_make && !full.camera_model && !full.lens && !full.lens_make && !full.software && full.focal_length === null && full.aperture === null)}>
            {full?.camera_make && <Metric label="Make" value={full.camera_make} />}
            {full?.camera_model && <Metric label="Model" value={full.camera_model} />}
            {full?.lens && <Metric label="Lens" value={full.lens} />}
            {full?.lens_make && <Metric label="Lens make" value={full.lens_make} />}
            {full?.software && <Metric label="Software" value={full.software} />}
            {full && full.focal_length != null && <Metric label="Focal length" value={fmtFocal(full.focal_length)} />}
            {full && full.aperture != null && <Metric label="Aperture" value={fmtAperture(full.aperture)} />}
            {full && full.iso != null && <Metric label="ISO" value={full.iso} />}
            {full && full.shutter_speed != null && <Metric label="Shutter" value={fmtShutter(full.shutter_speed)} />}
          </Section>

          <Section title="Analysis" empty={!analyzed}>
            {analyzed && full ? (
              <>
                <Metric label="Sharpness" value={full.sharpness?.toFixed(1) ?? "—"} />
                <Metric label="Brightness" value={full.brightness?.toFixed(1) ?? "—"} />
                <Metric label="Contrast" value={full.contrast?.toFixed(1) ?? "—"} />
                <Metric label="Saturation" value={full.saturation?.toFixed(2) ?? "—"} />
                <Metric label="Highlight clipping" value={`${(full.highlight_clipping ?? 0).toFixed(3)}`} />
                <Metric label="Shadow clipping" value={`${(full.shadow_clipping ?? 0).toFixed(3)}`} />
                {full.face_count != null && <Metric label="Faces" value={full.face_count} />}
              </>
            ) : (
              <div className="meta-empty">
                Not measured yet — the analysis pass for this photo has not run.
              </div>
            )}
          </Section>

          <div className="viewer-path mono faint" title={full?.path}>
            {full?.path ?? ""}
          </div>
        </aside>
      </div>
    </div>
  );
}
