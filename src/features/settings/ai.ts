/**
 * Wordings for the Settings "Local intelligence" card. Pure and testable —
 * the card composes these instead of formatting inline.
 */
import type { AiStatus, FaceSummary } from "@/types/api";

/** "232 KB" style size line for the embedded model. */
export function formatModelSize(bytes: number): string {
  if (bytes < 1000) return `${bytes} B`;
  const kb = bytes / 1024;
  if (kb < 1000) return `${kb < 10 && kb % 1 !== 0 ? kb.toFixed(1) : Math.round(kb)} KB`;
  return `${(kb / 1024).toFixed(1)} MB`;
}

/**
 * The "N of M photographed" line. Honest about what a face result is: a
 * count of detected faces, never a judgment.
 */
export function formatFacesProgressLine(status: AiStatus): string {
  if (status.photo_count === 0) {
    return "No photographs in the library yet.";
  }
  if (status.faces_done === 0) {
    return `${status.photo_count.toLocaleString()} photograph${
      status.photo_count === 1 ? "" : "s"
    } in the library, none checked for faces yet.`;
  }
  return `${status.faces_done.toLocaleString()} of ${status.photo_count.toLocaleString()} photographs checked for faces.`;
}

/** One line summarising the last face pass. */
export function formatFaceSummaryLine(summary: FaceSummary): string {
  const bits: string[] = [];
  if (summary.processed > 0) {
    bits.push(
      `checked ${summary.processed.toLocaleString()} photograph${summary.processed === 1 ? "" : "s"}`,
    );
  }
  bits.push(`${summary.with_faces.toLocaleString()} with ${summary.with_faces === 1 ? "a face" : "faces"}`);
  if (summary.failed > 0) bits.push(`${summary.failed.toLocaleString()} unreadable`);
  bits.push(`${(summary.elapsed_ms / 1000).toFixed(1)}s`);
  return (summary.cancelled ? "Face detection stopped — " : "Face detection complete — ") +
    bits.join(", ") +
    ".";
}

/** The card's availability line (friendly, never blamey). */
export function runtimeLine(status: AiStatus): string {
  if (status.runtime_available) return "Local runtime available on this machine.";
  return (
    status.runtime_note ??
    "The local ONNX Runtime is not available on this machine, so face detection is off for it. Everything else in PhotoGremlin works normally."
  );
}
