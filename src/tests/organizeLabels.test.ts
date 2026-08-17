import { describe, it, expect } from "vitest";
import { cleanName, groupLabel, NAME_MAX } from "@/features/organize/labels";

describe("cleanName (saved views + collections)", () => {
  it("trims surrounding whitespace and keeps inner spaces", () => {
    expect(cleanName("  Wedding selects  ")).toBe("Wedding selects");
  });

  it("rejects empty or whitespace-only input", () => {
    expect(cleanName("")).toBeNull();
    expect(cleanName("   \t ")).toBeNull();
  });

  it("rejects names longer than the cap (trailing spaces don't count)", () => {
    expect(cleanName("x".repeat(NAME_MAX + 1))).toBeNull();
    expect(cleanName("x".repeat(NAME_MAX))).toBe("x".repeat(NAME_MAX));
    // Trimming can bring an overlong name under the cap.
    expect(cleanName("  " + "x".repeat(NAME_MAX) + "  ")).toBe("x".repeat(NAME_MAX));
  });
});

describe("groupLabel (similarity group cards)", () => {
  it("labels similar groups factually, without verdicts", () => {
    expect(groupLabel("similar", 2)).toBe("2 similar");
    expect(groupLabel("similar", 5)).toBe("5 similar photographs");
  });

  it("labels bursts by size", () => {
    expect(groupLabel("burst", 1)).toBe("burst · 1");
    expect(groupLabel("burst", 4)).toBe("burst · 4");
  });

  it("treats an unknown type as a similar group", () => {
    expect(groupLabel("???", 3)).toBe("3 similar photographs");
  });
});
