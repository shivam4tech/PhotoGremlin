import { describe, it, expect } from "vitest";
import {
  FIELD_BY_NAME,
  FILTER_FIELDS,
  OPS_BY_KIND,
  buildCondition,
  chipLabel,
  draftToFilter,
  endOfDay,
  quickRangeBounds,
  quickRangeCondition,
  replaceFieldConditions,
  toggleExactFieldCondition,
  STANDARD_FILTER_STOPS,
} from "@/features/library/filterFields";

describe("filter registry", () => {
  it("mirrors the Rust field registry (names + kinds)", () => {
    const expectField = (field: string, kind: string, area: string) => {
      const def = FIELD_BY_NAME[field];
      expect(def, `missing field ${field}`).toBeDefined();
      expect(def?.kind).toBe(kind);
      expect(def?.area).toBe(area);
    };
    expectField("sharpness", "real", "Technical");
    expectField("brightness", "real", "Technical");
    expectField("contrast", "real", "Technical");
    expectField("saturation", "real", "Technical");
    expectField("highlight_clipping", "real", "Technical");
    expectField("shadow_clipping", "real", "Technical");
    expectField("monochrome", "bool", "Visual");
    expectField("color", "bool", "Visual");
    expectField("orientation", "text", "Orientation");
    expectField("camera_make", "text", "Camera & lens");
    expectField("camera_model", "text", "Camera & lens");
    expectField("lens", "text", "Camera & lens");
    expectField("iso", "int", "Exposure");
    expectField("aperture", "real", "Exposure");
    expectField("shutter_speed", "real", "Exposure");
    expectField("focal_length", "real", "Exposure");
    expectField("capture_datetime", "datetime", "Time");
    expectField("faces_present", "bool", "Faces & smiles (local models)");
    expectField("smiling", "bool", "Faces & smiles (local models)");
    expectField("rating", "int", "Marking");
    expectField("flagged", "bool", "Marking");
    expectField("color_label", "text", "Marking");
    expectField("review_state", "text", "Review");
  });

  it("exposes the right operators per kind", () => {
    expect(OPS_BY_KIND.bool.map((o) => o.op)).toEqual(["=", "!="]);
    expect(OPS_BY_KIND.real.map((o) => o.op)).toContain(">=");
    expect(OPS_BY_KIND.datetime.map((o) => o.op)).toContain("between");
    // text has no range operators
    expect(OPS_BY_KIND.text.map((o) => o.op)).not.toContain(">");
    // every registered field resolves to a known operator list
    for (const f of FILTER_FIELDS) {
      expect(OPS_BY_KIND[f.kind].length).toBeGreaterThan(0);
    }
  });

  it("orientation is a fixed value set", () => {
    expect(FIELD_BY_NAME.orientation.values).toEqual(["landscape", "portrait", "square"]);
  });

  it("marking fields: color_label enum and chips", () => {
    expect(FIELD_BY_NAME.color_label.values).toEqual([
      "red", "yellow", "green", "blue", "purple", "gray",
    ]);
    const c = buildCondition("color_label", "=", "green", "");
    expect(c).toEqual({ field: "color_label", operator: "=", value: "green" });
    expect(buildCondition("color_label", "=", "mauve", "")).toBeNull();
    expect(chipLabel(buildCondition("rating", "is-null", "", "")!)).toBe("rating: not recorded");
    expect(chipLabel(buildCondition("flagged", "=", "true", "")!)).toBe("flagged");
  });
});

describe("endOfDay", () => {
  it("extends bare dates to end-of-day UTC", () => {
    expect(endOfDay("2026-08-15")).toBe("2026-08-15T23:59:59Z");
  });
  it("passes full timestamps through unchanged", () => {
    expect(endOfDay("2026-08-15T10:00:00Z")).toBe("2026-08-15T10:00:00Z");
  });
});

describe("buildCondition", () => {
  it("builds a numeric comparison", () => {
    expect(buildCondition("sharpness", ">=", "70", "")).toEqual({
      field: "sharpness",
      operator: ">=",
      value: 70,
    });
  });

  it("rejects non-numeric values for real fields", () => {
    expect(buildCondition("sharpness", "=", "nope", "")).toBeNull();
  });

  it("keeps ISO whole (rejects fractions)", () => {
    expect(buildCondition("iso", "<", "1600", "")).toEqual({
      field: "iso",
      operator: "<",
      value: 1600,
    });
    expect(buildCondition("iso", "<", "1.5", "")).toBeNull();
  });

  it("builds between with two values", () => {
    expect(buildCondition("aperture", "between", "1.4", "4")).toEqual({
      field: "aperture",
      operator: "between",
      value: [1.4, 4],
    });
    expect(buildCondition("aperture", "between", "1.4", "")).toBeNull();
  });

  it("builds in-lists as arrays", () => {
    expect(buildCondition("camera_model", "in", "A7, R5", "")).toEqual({
      field: "camera_model",
      operator: "in",
      value: ["A7", "R5"],
    });
    expect(buildCondition("camera_model", "in", "   ", "")).toBeNull();
  });

  it("honors fixed value sets (orientation)", () => {
    expect(buildCondition("orientation", "=", "portrait", "")).toEqual({
      field: "orientation",
      operator: "=",
      value: "portrait",
    });
    expect(buildCondition("orientation", "=", "diagonal", "")).toBeNull();
  });

  it("builds null-state operators without a value", () => {
    expect(buildCondition("lens", "is-null", "", "")).toEqual({
      field: "lens",
      operator: "is-null",
      value: null,
    });
  });

  it("builds booleans from the select's true/false strings", () => {
    expect(buildCondition("monochrome", "=", "true", "")).toEqual({
      field: "monochrome",
      operator: "=",
      value: true,
    });
    expect(buildCondition("monochrome", "!=", "false", "")).toEqual({
      field: "monochrome",
      operator: "!=",
      value: false,
    });
  });

  it("datetime between auto-extends the upper bound to end of day", () => {
    expect(buildCondition("capture_datetime", "between", "2026-01-01", "2026-12-31")).toEqual({
      field: "capture_datetime",
      operator: "between",
      value: ["2026-01-01", "2026-12-31T23:59:59Z"],
    });
  });

  it("unknown fields build nothing", () => {
    expect(buildCondition("vibes", "=", "1", "")).toBeNull();
  });
});

describe("chipLabel", () => {
  it("uses neutral technical language", () => {
    expect(chipLabel({ field: "sharpness", operator: ">=", value: 70 })).toBe("sharpness ≥ 70");
    expect(chipLabel({ field: "iso", operator: "<", value: 1600 })).toBe("iso < 1600");
  });

  it("labels measured ranges with their exact numeric bounds", () => {
    expect(chipLabel({ field: "brightness", operator: "<=", value: 35 })).toBe(
      "brightness ≤ 35",
    );
    expect(chipLabel({ field: "sharpness", operator: "between", value: [40, 70] })).toBe(
      "sharpness 40 → 70",
    );
  });

  it("phrases booleans as properties, not verdicts", () => {
    expect(chipLabel({ field: "monochrome", operator: "=", value: true })).toBe("monochrome");
    expect(chipLabel({ field: "faces_present", operator: "=", value: true })).toBe(
      "contains faces",
    );
    expect(chipLabel({ field: "color", operator: "!=", value: false })).toBe("not in color");
  });

  it("shows date ranges without the hidden end-of-day", () => {
    expect(
      chipLabel({
        field: "capture_datetime",
        operator: "between",
        value: ["2026-01-01", "2026-12-31T23:59:59Z"],
      }),
    ).toBe("capture date 2026-01-01 → 2026-12-31");
  });

  it("renders in-lists and null states", () => {
    expect(chipLabel({ field: "lens", operator: "is-null", value: null })).toBe(
      "lens: not recorded",
    );
    expect(chipLabel({ field: "camera_model", operator: "in", value: ["A7", "R5"] })).toBe(
      "camera model in {A7, R5}",
    );
  });
});

describe("quick filter controls", () => {
  it("maps range bounds onto ordinary inclusive filter conditions", () => {
    expect(quickRangeCondition("brightness", 0, 100, 0, 100)).toBeNull();
    expect(quickRangeCondition("brightness", 35, 100, 0, 100)).toEqual({
      field: "brightness", operator: ">=", value: 35,
    });
    expect(quickRangeCondition("sharpness", 40, 70, 0, 100)).toEqual({
      field: "sharpness", operator: "between", value: [40, 70],
    });
    expect(quickRangeCondition("iso", 25, 1600, 25, 102400)).toEqual({
      field: "iso", operator: "<=", value: 1600,
    });
  });

  it("reads current and legacy bounds without rewriting their condition", () => {
    expect(quickRangeBounds({ field: "brightness", operator: "<", value: 35 }, 0, 100))
      .toEqual({ lower: 0, upper: 35, missingOnly: false, editable: true });
    expect(quickRangeBounds({ field: "sharpness", operator: "between", value: [40, 70] }, 0, 100))
      .toEqual({ lower: 40, upper: 70, missingOnly: false, editable: true });
    expect(quickRangeBounds({ field: "contrast", operator: "is-null", value: null }, 0, 100))
      .toEqual({ lower: 0, upper: 100, missingOnly: true, editable: true });
    expect(quickRangeBounds({ field: "iso", operator: "in", value: [100, 400] }, 25, 102400).editable)
      .toBe(false);
  });

  it("owns only the selected field", () => {
    const initial = [
      { field: "brightness", operator: ">=" as const, value: 20 },
      { field: "iso", operator: "<" as const, value: 1600 },
    ];
    const next = replaceFieldConditions(
      initial,
      "brightness",
      quickRangeCondition("brightness", 65, 100, 0, 100),
    );
    expect(next).toEqual([
      { field: "iso", operator: "<", value: 1600 },
      { field: "brightness", operator: ">=", value: 65 },
    ]);
  });

  it("uses familiar, increasing ISO and focal-length stops", () => {
    expect(STANDARD_FILTER_STOPS.iso).toContain(1600);
    expect(STANDARD_FILTER_STOPS.focal_length).toEqual(expect.arrayContaining([24, 35, 50, 85, 200]));
    expect([...STANDARD_FILTER_STOPS.iso]).toEqual([...STANDARD_FILTER_STOPS.iso].sort((a, b) => a - b));
  });

  it("supports an exact inclusive range when both handles meet", () => {
    expect(quickRangeCondition("focal_length", 85, 85, 8, 1200)).toEqual({
      field: "focal_length", operator: "between", value: [85, 85],
    });
  });

  it("toggles a review shortcut without discarding unrelated filters", () => {
    const iso = { field: "iso", operator: "<=", value: 1600 } as const;
    const unreviewed = { field: "review_state", operator: "is-null", value: null } as const;
    const kept = { field: "review_state", operator: "=", value: "selected" } as const;

    expect(toggleExactFieldCondition([iso], unreviewed)).toEqual([iso, unreviewed]);
    expect(toggleExactFieldCondition([iso, unreviewed], kept)).toEqual([iso, kept]);
    expect(toggleExactFieldCondition([iso, kept], kept)).toEqual([iso]);
  });
});

describe("draftToFilter", () => {
  it("emits the exact wire object the Rust engine parses", () => {
    const j = JSON.stringify(
      draftToFilter([
        { field: "sharpness", operator: ">=", value: 70 },
        { field: "orientation", operator: "=", value: "portrait" },
      ]),
    );
    expect(j).toBe(
      '{"operator":"AND","conditions":[{"field":"sharpness","operator":">=","value":70},{"field":"orientation","operator":"=","value":"portrait"}]}',
    );
  });
});
