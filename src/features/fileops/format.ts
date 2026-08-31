/**
 * Pure presentation helpers for the file-operations panel (Sprint 7).
 * Kept separate from the React component so the wording (which must stay
 * factual — "3 moved", never "3 photos saved!") is unit-testable.
 */
import type { FileOpPlan, OperationSummary, ProgressPayload } from "@/types/api";

/** Trailing path segment for previews: `/a/b/IMG_0001.jpg` → `IMG_0001.jpg`. */
export function fileBase(p: string | null): string {
  if (!p) return "—";
  const parts = p.split("/");
  return parts[parts.length - 1] || p;
}

/** Factual verb for an operation preview: what will happen to the files. */
export function opVerb(op: FileOpPlan["op"]): string {
  switch (op) {
    case "rename":
      return "be renamed";
    case "move":
      return "be moved";
    case "copy":
      return "be copied";
    case "trash":
      return "be trashed";
    case "delete-permanently":
      return "be permanently deleted";
  }
}

/** Preview headline: "Preview: 8 of 9 will be moved". */
export function previewHeadline(plan: FileOpPlan): string {
  const ok = plan.items.filter((i) => i.ok).length;
  if (plan.aborted) {
    return "Plan aborted — two or more files would get the same name";
  }
  return `Preview: ${ok.toLocaleString()} of ${plan.items.length.toLocaleString()} will ${opVerb(plan.op)}`;
}

/** Result headline: "3 of 5 move complete". */
export function resultHeadline(summary: OperationSummary): string {
  return `${summary.succeeded.toLocaleString()} of ${summary.total.toLocaleString()} ${summary.op} complete`;
}

/**
 * Progress line while an operation runs, or from the final summary when the
 * progress event has already stopped. Never invents a total.
 */
export function progressLabel(
  opProgress: ProgressPayload | null,
  summary: OperationSummary | null,
): string {
  const done = opProgress ? opProgress.done : summary ? summary.processed : 0;
  const total = opProgress ? opProgress.total : summary ? summary.total : 0;
  return total > 0 ? `${done.toLocaleString()} / ${total.toLocaleString()} items` : "working…";
}

/** Which items need explanation in the results: everything that is not done. */
export function flaggedResults(summary: OperationSummary): OperationSummary["items"] {
  return summary.items.filter((i) => i.status !== "done");
}
