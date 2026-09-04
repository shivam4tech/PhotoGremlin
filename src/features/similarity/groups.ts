import type { GroupPhotoSort, PhotoSummary, SimilarityGroup } from "@/types/api";

export type GroupTab = "all" | SimilarityGroup["group_type"];

export function groupsForTab(groups: SimilarityGroup[], tab: GroupTab): SimilarityGroup[] {
  return tab === "all" ? groups : groups.filter((group) => group.group_type === tab);
}

export function mergeGroupPhotos(
  current: PhotoSummary[],
  incoming: PhotoSummary[],
  offset: number,
): PhotoSummary[] {
  if (offset === 0) return incoming;
  const seen = new Set(current.map((photo) => photo.id));
  return [...current, ...incoming.filter((photo) => !seen.has(photo.id))];
}

export function groupDescription(groupType: SimilarityGroup["group_type"]): string {
  if (groupType === "burst") return "Captured within seconds of each other";
  if (groupType === "face") return "Similar locally measured face appearance — not an identity label";
  return "Near-identical visual structure";
}

export const GROUP_SORT_OPTIONS: readonly { value: GroupPhotoSort; label: string }[] = [
  { value: "chronology", label: "Capture time" },
  { value: "sharpness_desc", label: "Sharpness: highest first" },
  { value: "clipping_asc", label: "Clipping: lowest first" },
  { value: "eyes_open_first", label: "Eyes open first" },
];

export function groupCaptureSignals(group: SimilarityGroup): string[] {
  const signals: string[] = [];
  if (group.possible_blink_count > 0) {
    signals.push(`${group.possible_blink_count} possible blink${group.possible_blink_count === 1 ? "" : "s"}`);
  }
  if (group.closed_eye_candidate_count > 0) {
    signals.push(`${group.closed_eye_candidate_count} closed-eye candidate${group.closed_eye_candidate_count === 1 ? "" : "s"}`);
  }
  if (group.unevaluated_eye_count > 0) {
    signals.push(`${group.unevaluated_eye_count} without eye measurements`);
  }
  return signals;
}
