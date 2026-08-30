import type { PhotoSummary, ReviewSequence } from "@/types/api";
import type { SelectionState } from "@/stores/appStore";

export type ReviewUnitKind = "burst" | "similar" | "single";

/** A small, chronological decision context; each photograph appears once. */
export interface ReviewUnit {
  key: string;
  kind: ReviewUnitKind;
  photoIds: number[];
  firstPhotoIndex: number;
}

function priority(kind: ReviewUnitKind): number {
  return kind === "burst" ? 0 : kind === "similar" ? 1 : 2;
}

/**
 * Turn overlapping similarity output into a calm, non-repeating review
 * sequence. Bursts win overlaps because they preserve the photographer's
 * temporal context; everything else remains in capture-time order.
 */
export function buildReviewUnits(
  photos: PhotoSummary[],
  sequences: ReviewSequence[],
): ReviewUnit[] {
  const photoIndex = new Map(photos.map((photo, index) => [photo.id, index]));
  const used = new Set<number>();
  const units: ReviewUnit[] = [];

  const orderedSequences = sequences
    .map((sequence) => ({
      ...sequence,
      photoIds: [...new Set(sequence.photo_ids)].filter((id) => photoIndex.has(id)),
    }))
    .filter((sequence) => sequence.photoIds.length >= 2)
    .sort((left, right) => {
      const leftKind = left.group_type === "burst" ? "burst" : "similar";
      const rightKind = right.group_type === "burst" ? "burst" : "similar";
      return priority(leftKind) - priority(rightKind)
        || (photoIndex.get(left.photoIds[0]) ?? 0) - (photoIndex.get(right.photoIds[0]) ?? 0)
        || left.id - right.id;
    });

  for (const sequence of orderedSequences) {
    const photoIds = sequence.photoIds.filter((id) => !used.has(id));
    if (photoIds.length < 2) continue;
    photoIds.forEach((id) => used.add(id));
    const kind = sequence.group_type === "burst" ? "burst" : "similar";
    units.push({
      key: `${kind}:${sequence.id}`,
      kind,
      photoIds,
      firstPhotoIndex: photoIndex.get(photoIds[0]) ?? 0,
    });
  }

  for (const photo of photos) {
    if (!used.has(photo.id)) {
      units.push({ key: `single:${photo.id}`, kind: "single", photoIds: [photo.id], firstPhotoIndex: photoIndex.get(photo.id) ?? 0 });
    }
  }

  return units.sort((left, right) => left.firstPhotoIndex - right.firstPhotoIndex || priority(left.kind) - priority(right.kind));
}

export function firstUnreviewedId(unit: ReviewUnit, selections: Record<number, SelectionState>): number {
  return unit.photoIds.find((id) => !selections[id]) ?? unit.photoIds[0];
}

/** Keep comparison bounded while preserving the sequence's capture order. */
export function comparisonPhotoIds(photoIds: number[], focusedId: number, limit = 4): number[] {
  if (limit <= 0 || photoIds.length === 0) return [];
  const boundedLimit = Math.min(limit, photoIds.length);
  const focusIndex = Math.max(0, photoIds.indexOf(focusedId));
  const idealStart = focusIndex - Math.floor((boundedLimit - 1) / 2);
  const start = Math.max(0, Math.min(photoIds.length - boundedLimit, idealStart));
  return photoIds.slice(start, start + boundedLimit);
}

export function reviewCounts(photoIds: number[], selections: Record<number, SelectionState>) {
  let selected = 0;
  let rejected = 0;
  let needsAttention = 0;
  for (const id of photoIds) {
    if (selections[id] === "selected") selected += 1;
    if (selections[id] === "rejected") rejected += 1;
    if (selections[id] === "needs_attention") needsAttention += 1;
  }
  return { selected, rejected, needsAttention, reviewed: selected + rejected + needsAttention };
}
