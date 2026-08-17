import { describe, it, expect } from "vitest";
import {
  FIELD_BY_NAME,
  FILTER_FIELDS,
  OPS_BY_KIND,
  buildCondition,
  chipLabel,
  draftToFilter,
  endOfDay,
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
