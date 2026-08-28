import type { PhotoSummary, SimilarityGroup } from "@/types/api";

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
