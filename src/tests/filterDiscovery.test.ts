import { describe, expect, it } from "vitest";
import { FILTER_FIELDS, chipLabel } from "@/features/library/filterFields";
import { searchFilterChoices, savedViewMatches } from "@/features/library/filterDiscovery";
import type { FilterCondition } from "@/types/api";

describe("filter discovery", () => {
  it("keeps every registered field discoverable by its exact name", () => {
    for (const field of FILTER_FIELDS) {
      expect(searchFilterChoices(field.field.replaceAll("_", " ")).some((choice) => choice.id === field.field)).toBe(true);
    }
  });
  it("finds technical fields and related people shortcuts", () => {
    expect(searchFilterChoices("  ISO ").some((choice) => choice.id === "iso")).toBe(true);
    expect(searchFilterChoices("sharp").some((choice) => choice.id === "sharpness")).toBe(true);
    const faces = searchFilterChoices("face").map((choice) => choice.label);
    expect(faces).toContain("Contains faces");
    expect(faces).toContain("Possible blink");
    expect(faces).toContain("Closed-eye candidate");
    expect(searchFilterChoices("not-a-real-filter")).toEqual([]);
  });
  it("matches reordered saved AND conditions without changing range semantics", () => {
    const draft: FilterCondition[] = [
      { field: "iso", operator: "between", value: [100, 800] },
      { field: "palette_color", operator: "in", value: ["yellow", "blue"] },
    ];
    const saved = JSON.stringify({ operator: "AND", conditions: [
      { ...draft[1], value: ["blue", "yellow"] }, draft[0],
    ] });
    expect(savedViewMatches(saved, draft)).toBe(true);
    expect(savedViewMatches(saved, [{ ...draft[0], value: [800, 100] }, draft[1]])).toBe(false);
    expect(savedViewMatches("{}", draft)).toBe(false);
    expect(savedViewMatches("invalid", draft)).toBe(false);
  });
  it("labels negative boolean filters honestly", () => {
    expect(chipLabel({ field: "faces_present", operator: "=", value: false })).toMatch(/^Not /i);
    expect(chipLabel({ field: "faces_present", operator: "!=", value: false })).not.toMatch(/^Not /i);
  });
});
