import { describe, expect, it } from "vitest";
import {
  fmtAperture,
  fmtDuration,
  fmtIso,
  fmtMetric,
  fmtRatio,
  fmtShare,
  fmtShutter,
  maxCount,
  monthLabel,
} from "@/features/stats/format";
import { periodJson } from "@/types/api";

describe("fmtMetric", () => {
  it("renders one decimal", () => {
    expect(fmtMetric(61.6666)).toBe("61.7");
    expect(fmtMetric(0)).toBe("0.0");
    expect(fmtMetric(100)).toBe("100.0");
  });

  it("never renders null as zero (honest unavailable)", () => {
    expect(fmtMetric(null)).toBe("unavailable");
  });
});

describe("fmtShare / fmtRatio", () => {
  it("renders percent without decimals", () => {
    expect(fmtShare(33.333)).toBe("33%");
    expect(fmtShare(100)).toBe("100%");
  });

  it("renders 0-1 ratios as percent", () => {
    expect(fmtRatio(0.25)).toBe("25%");
    expect(fmtRatio(1)).toBe("100%");
  });

  it("renders null as unavailable, not 0%", () => {
    expect(fmtShare(null)).toBe("unavailable");
    expect(fmtRatio(null)).toBe("unavailable");
  });
});

describe("fmtDuration", () => {
  it("renders days with one decimal", () => {
    expect(fmtDuration(5.208333)).toBe("5.2 days");
    expect(fmtDuration(10)).toBe("10 days");
  });

  it("renders sub-day durations as hours", () => {
    expect(fmtDuration(0.5 + 0.5 / 24)).toBe("13 hours");
    expect(fmtDuration(0.01)).toBe("< 1 hour");
  });

  it("renders null as unknown", () => {
    expect(fmtDuration(null)).toBe("unknown");
  });
});

describe("EXIF formatters", () => {
  it("ISO rounds to an integer", () => {
    expect(fmtIso(249.5)).toBe("250");
    expect(fmtIso(null)).toBe("unavailable");
  });

  it("aperture uses f/ prefix", () => {
    expect(fmtAperture(2.8)).toBe("f/2.8");
    expect(fmtAperture(5.6)).toBe("f/5.6");
    expect(fmtAperture(null)).toBe("unavailable");
  });

  it("shutter renders 1/N when close, seconds otherwise", () => {
    expect(fmtShutter(0.5)).toBe("1/2");
    expect(fmtShutter(1 / 125)).toBe("1/125");
    expect(fmtShutter(1)).toBe("1s");
    expect(fmtShutter(0.6)).toBe("0.600s");
    expect(fmtShutter(null)).toBe("unavailable");
  });
});

describe("monthLabel / maxCount", () => {
  it("labels YYYY-MM months", () => {
    expect(monthLabel("2026-01")).toBe("Jan 2026");
    expect(monthLabel("2026-06")).toBe("Jun 2026");
    expect(monthLabel("2026-12")).toBe("Dec 2026");
  });

  it("passes through unparseable strings", () => {
    expect(monthLabel("garbage")).toBe("garbage");
  });

  it("finds the max bin count", () => {
    expect(maxCount([{ count: 3 }, { count: 9 }, { count: 1 }])).toBe(9);
    expect(maxCount([])).toBe(0);
  });
});

describe("periodJson", () => {
  it("encodes the one period model", () => {
    expect(periodJson("today")).toBe('{"kind":"today"}');
    expect(periodJson("this-week")).toBe('{"kind":"this-week"}');
    expect(periodJson("this-month")).toBe('{"kind":"this-month"}');
    expect(periodJson("this-year")).toBe('{"kind":"this-year"}');
    expect(periodJson("unknown-kind")).toBe('{"kind":"all"}');
  });

  it("encodes custom ranges", () => {
    expect(periodJson("custom", "2026-05-01", "2026-05-31")).toBe(
      '{"kind":"custom","from":"2026-05-01","to":"2026-05-31"}',
    );
  });
});
