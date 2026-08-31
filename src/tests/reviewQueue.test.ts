import { describe, expect, it } from "vitest";
import {
  buildReviewUnits,
  comparisonPhotoIds,
  firstUnreviewedId,
  reviewCounts,
} from "@/features/review/reviewQueue";
import type { PhotoSummary, ReviewSequence } from "@/types/api";

const photos: PhotoSummary[] = [1, 2, 3, 4, 5].map((id) => ({
  id,
  filename: `${id}.jpg`,
  extension: "jpg",
  size_bytes: null,
  width: null,
  height: null,
  orientation: null,
  capture_datetime: null,
  session_id: 1,
  has_analysis: false,
  rating: null,
  flag: false,
  color_label: null,
  sharpness: null,
  highlight_clipping: null,
  shadow_clipping: null,
  closed_eye_face_count: null,
  max_eye_closure_confidence: null,
}));

describe("shoot review queue", () => {
  it("keeps burst context, removes overlaps, and leaves every photo once", () => {
    const sequences: ReviewSequence[] = [
      { id: 2, group_type: "similar", photo_ids: [2, 3] },
      { id: 1, group_type: "burst", photo_ids: [1, 2, 3] },
    ];
    const units = buildReviewUnits(photos, sequences);
    expect(units.map((unit) => unit.photoIds)).toEqual([[1, 2, 3], [4], [5]]);
    expect(units[0].kind).toBe("burst");
  });

  it("reports decisions without treating them as an automatic verdict", () => {
    const unit = buildReviewUnits(photos, [])[0];
    expect(firstUnreviewedId(unit, { 1: "selected" })).toBe(1);
    expect(reviewCounts([1, 2, 3], { 1: "selected", 2: "rejected", 3: "needs_attention" })).toEqual({
      selected: 1,
      rejected: 1,
      needsAttention: 1,
      reviewed: 3,
    });
  });

  it("keeps a bounded chronological window around the focused comparison frame", () => {
    const ids = [10, 11, 12, 13, 14, 15];
    expect(comparisonPhotoIds(ids, 10)).toEqual([10, 11, 12, 13]);
    expect(comparisonPhotoIds(ids, 13)).toEqual([12, 13, 14, 15]);
    expect(comparisonPhotoIds(ids, 15)).toEqual([12, 13, 14, 15]);
    expect(comparisonPhotoIds(ids, 12, 2)).toEqual([12, 13]);
    expect(comparisonPhotoIds(ids, 12, 0)).toEqual([]);
  });
});
