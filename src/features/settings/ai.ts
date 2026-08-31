/**
 * Wordings for the Settings "Local intelligence" card. Pure and testable —
 * the card composes these instead of formatting inline.
 */
import type { AiStatus, FaceSummary, SceneSummary } from "@/types/api";

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

/** Eye-state results are produced by the same local pass as face counts. */
export function formatEyesProgressLine(status: AiStatus): string {
  if (status.photo_count === 0) return "No photographs in the library yet.";
  if (status.eyes_done === 0) {
    return `${status.photo_count.toLocaleString()} photograph${
      status.photo_count === 1 ? "" : "s"
    } in the library, none checked for eye state yet.`;
  }
  return `${status.eyes_done.toLocaleString()} of ${status.photo_count.toLocaleString()} photographs checked for eye state.`;
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
  bits.push(`${summary.eyes_evaluated.toLocaleString()} eyes evaluated`);
  if (summary.closed_eye_faces > 0) {
    bits.push(`${summary.closed_eye_faces.toLocaleString()} closed-eye face candidate${summary.closed_eye_faces === 1 ? "" : "s"}`);
  }
  if (summary.failed > 0) bits.push(`${summary.failed.toLocaleString()} unreadable`);
  bits.push(`${(summary.elapsed_ms / 1000).toFixed(1)}s`);
  return (summary.cancelled ? "Face and eye-state analysis stopped — " : "Face and eye-state analysis complete — ") +
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

/** The scene card's "N of M" line (Sprint 18). */
export function formatScenesProgressLine(status: AiStatus): string {
  if (status.photo_count === 0) {
    return "No photographs in the library yet.";
  }
  if (status.scenes_done === 0) {
    return `${status.photo_count.toLocaleString()} photograph${
      status.photo_count === 1 ? "" : "s"
    } in the library, none classified yet.`;
  }
  return `${status.scenes_done.toLocaleString()} of ${status.photo_count.toLocaleString()} photographs classified.`;
}

/** One line summarising the last scene pass. */
export function formatSceneSummaryLine(summary: SceneSummary): string {
  const bits: string[] = [];
  if (summary.processed > 0) {
    bits.push(
      `classified ${summary.processed.toLocaleString()} photograph${summary.processed === 1 ? "" : "s"}`,
    );
  }
  if (summary.failed > 0) bits.push(`${summary.failed.toLocaleString()} unreadable`);
  bits.push(`${(summary.elapsed_ms / 1000).toFixed(1)}s`);
  return (summary.cancelled ? "Scene classification stopped — " : "Scene classification complete — ") +
    bits.join(", ") +
    ".";
}
