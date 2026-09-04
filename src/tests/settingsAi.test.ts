import { describe, it, expect } from "vitest";
import {
  formatModelSize,
  formatFacesProgressLine,
  formatEyesProgressLine,
  formatFaceSummaryLine,
  runtimeLine,
} from "@/features/settings/ai";
import type { AiStatus, FaceSummary } from "@/types/api";

const baseStatus: AiStatus = {
  enabled: false,
  runtime_available: true,
  runtime_note: null,
  model: "YuNet 2023mar (OpenCV Zoo, Apache-2.0)",
  model_bytes: 223253,
  eye_model: "OCEC-S (MIT)",
  eye_model_bytes: 494000,
  faces_done: 0,
  eyes_done: 0,
  scene_model: "MobileNetV3-Large two-head (trained on CC-BY Open Images)",
  scene_model_bytes: 10240,
  scenes_done: 0,
  photo_count: 0,
};

const baseSummary: FaceSummary = {
  processed: 0,
  with_faces: 0,
  eyes_evaluated: 0,
  closed_eye_faces: 0,
  failed: 0,
  cancelled: false,
  elapsed_ms: 0,
  errors: [],
};

describe("formatModelSize", () => {
  it("formats bytes, kilobytes and megabytes", () => {
    expect(formatModelSize(512)).toBe("512 B");
    expect(formatModelSize(232 * 1024)).toBe("232 KB");
    expect(formatModelSize(1500)).toBe("1.5 KB");
    expect(formatModelSize(5 * 1024 * 1024)).toBe("5.0 MB");
  });
});

describe("formatFacesProgressLine", () => {
  it("is honest when there is nothing checked yet", () => {
    expect(formatFacesProgressLine(baseStatus)).toBe("No photographs in the library yet.");
    expect(
      formatFacesProgressLine({ ...baseStatus, photo_count: 3 }),
    ).toBe("3 photographs in the library, none checked for faces yet.");
    expect(
      formatFacesProgressLine({ ...baseStatus, photo_count: 1 }),
    ).toBe("1 photograph in the library, none checked for faces yet.");
  });

  it("counts what has been checked, without judgment", () => {
    expect(
      formatFacesProgressLine({ ...baseStatus, photo_count: 1000, faces_done: 42 }),
    ).toBe("42 of 1,000 photographs checked for faces.");
  });
});

describe("formatEyesProgressLine", () => {
  it("reports eye-state coverage separately", () => {
    expect(formatEyesProgressLine({ ...baseStatus, photo_count: 3 })).toBe(
      "3 photographs in the library, none checked for eye state yet.",
    );
    expect(formatEyesProgressLine({ ...baseStatus, photo_count: 100, eyes_done: 24 })).toBe(
      "24 of 100 photographs checked for eye state.",
    );
  });
});

describe("formatFaceSummaryLine", () => {
  it("summarises a completed pass", () => {
    expect(
      formatFaceSummaryLine({
        ...baseSummary,
        processed: 120,
        with_faces: 30,
        eyes_evaluated: 82,
        closed_eye_faces: 3,
        failed: 2,
        elapsed_ms: 1234,
      }),
    ).toBe(
      "Face and eye-state analysis complete — checked 120 photographs, 30 with faces, 82 eyes evaluated, 3 closed-eye face candidates, 2 unreadable, 1.2s.",
    );
  });

  it("is singular and marks cancellations", () => {
    expect(
      formatFaceSummaryLine({ ...baseSummary, processed: 1, with_faces: 1, elapsed_ms: 800 }),
    ).toBe("Face and eye-state analysis complete — checked 1 photograph, 1 with a face, 0 eyes evaluated, 0.8s.");
    expect(
      formatFaceSummaryLine({ ...baseSummary, processed: 5, cancelled: true, elapsed_ms: 100 }),
    ).toBe("Face and eye-state analysis stopped — checked 5 photographs, 0 with faces, 0 eyes evaluated, 0.1s.");
  });
});

describe("runtimeLine", () => {
  it("reports availability and a friendly note when missing", () => {
    expect(runtimeLine(baseStatus)).toBe("Local runtime available on this machine.");
    expect(
      runtimeLine({ ...baseStatus, runtime_available: false, runtime_note: "no runtime here" }),
    ).toBe("no runtime here");
    expect(runtimeLine({ ...baseStatus, runtime_available: false }).length).toBeGreaterThan(20);
  });
});
