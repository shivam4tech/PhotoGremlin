import { describe, expect, it } from "vitest";
import {
  GROUP_SORT_OPTIONS,
  groupCaptureSignals,
  groupDescription,
  groupsForTab,
  mergeGroupPhotos,
} from "@/features/similarity/groups";
import type { PhotoSummary, SimilarityGroup } from "@/types/api";

const groups: SimilarityGroup[] = [
  { id: 1, hash: "a", group_type: "similar", photo_count: 2, created_at: "", session_count: 1, possible_blink_count: 0, closed_eye_candidate_count: 0, unevaluated_eye_count: 2, cover_photos: [] },
  { id: 2, hash: "b", group_type: "burst", photo_count: 4, created_at: "", session_count: 1, possible_blink_count: 1, closed_eye_candidate_count: 2, unevaluated_eye_count: 0, cover_photos: [] },
  { id: 3, hash: "c", group_type: "face", photo_count: 3, created_at: "", session_count: 2, possible_blink_count: 0, closed_eye_candidate_count: 0, unevaluated_eye_count: 0, cover_photos: [] },
];

describe("Groups workspace helpers", () => {
  it("filters each factual group type without changing all-group order", () => {
    expect(groupsForTab(groups, "all")).toEqual(groups);
    expect(groupsForTab(groups, "similar").map((group) => group.id)).toEqual([1]);
    expect(groupsForTab(groups, "burst").map((group) => group.id)).toEqual([2]);
    expect(groupsForTab(groups, "face").map((group) => group.id)).toEqual([3]);
  });

  it("replaces the first page and de-duplicates later pages", () => {
    const photo = (id: number) => ({ id } as PhotoSummary);
    expect(mergeGroupPhotos([photo(9)], [photo(1), photo(2)], 0).map((item) => item.id)).toEqual([1, 2]);
    expect(mergeGroupPhotos([photo(1), photo(2)], [photo(2), photo(3)], 2).map((item) => item.id)).toEqual([1, 2, 3]);
  });

  it("keeps group language measurable and local", () => {
    expect(groupDescription("similar")).toContain("visual structure");
    expect(groupDescription("burst")).toContain("seconds");
    expect(groupDescription("face")).toContain("not an identity label");
  });

  it("describes group capture signals and exposes every supported ordering", () => {
    expect(groupCaptureSignals(groups[1])).toEqual([
      "1 possible blink",
      "2 closed-eye candidates",
    ]);
    expect(groupCaptureSignals(groups[0])).toEqual(["2 without eye measurements"]);
    expect(GROUP_SORT_OPTIONS.map((option) => option.value)).toEqual([
      "chronology",
      "sharpness_desc",
      "clipping_asc",
      "eyes_open_first",
    ]);
  });
});
