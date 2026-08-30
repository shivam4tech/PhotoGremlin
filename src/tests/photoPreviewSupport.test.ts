import { describe, expect, it } from "vitest";
import { isPreviewable } from "@/components/PhotoTile";

describe("local preview support", () => {
  it("routes camera RAW files to the Rust preview ladder", () => {
    for (const extension of ["CR2", "cr3", "NEF", "ArW", "RAF", "DNG", "ORF", "RW2"]) {
      expect(isPreviewable(extension)).toBe(true);
    }
  });

  it("keeps formats without a local decoder on the placeholder path", () => {
    expect(isPreviewable("HEIC")).toBe(false);
    expect(isPreviewable("heif")).toBe(false);
    expect(isPreviewable("jpeg")).toBe(true);
  });
});
