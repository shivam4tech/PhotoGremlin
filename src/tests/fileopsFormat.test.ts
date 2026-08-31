import { describe, expect, it } from "vitest";
import {
  fileBase,
  flaggedResults,
  opVerb,
  previewHeadline,
  progressLabel,
  resultHeadline,
} from "@/features/fileops/format";
import type {
  OperationItemResult,
  OperationSummary,
  PlanItem,
} from "@/types/api";

function item(over: Partial<PlanItem> = {}): PlanItem {
  return {
    photo_id: 1,
    source: "/a/b/IMG_0001.jpg",
    destination: "/c/d/IMG_0001.jpg",
    note: null,
    ok: true,
    ...over,
  };
}

function opItem(over: Partial<OperationItemResult> = {}): OperationItemResult {
  return {
    source: "/a/b/IMG_0001.jpg",
    destination: "/c/d/IMG_0001.jpg",
    status: "done",
    detail: null,
    ...over,
  };
}

function summary(over: Partial<OperationSummary> = {}): OperationSummary {
  return {
    op: "move",
    total: 2,
    processed: 2,
    succeeded: 2,
    failed: 0,
    cancelled: false,
    elapsed_ms: 1200,
    items: [opItem(), opItem({ source: "/a/b/x.jpg" })],
    ...over,
  };
}

describe("fileBase", () => {
  it("returns the trailing segment", () => {
    expect(fileBase("/a/b/IMG_0001.jpg")).toBe("IMG_0001.jpg");
    expect(fileBase("plain.jpg")).toBe("plain.jpg");
  });

  it("handles null and windows-style paths", () => {
    expect(fileBase(null)).toBe("—");
    expect(fileBase("C:/x/y.jpg")).toBe("y.jpg");
  });
});

describe("opVerb / previewHeadline", () => {
  it("uses factual verbs", () => {
    expect(opVerb("rename")).toBe("be renamed");
    expect(opVerb("move")).toBe("be moved");
    expect(opVerb("copy")).toBe("be copied");
    expect(opVerb("trash")).toBe("be trashed");
    expect(opVerb("delete-permanently")).toBe("be permanently deleted");
  });

  it("counts only the items that will run", () => {
    const plan = {
      op: "move" as const,
      items: [item(), item({ photo_id: 2, ok: false, note: "ALREADY EXISTS" })],
      aborted: false,
      will_create_dir: null,
      destructive: false,
    };
    expect(previewHeadline(plan)).toBe("Preview: 1 of 2 will be moved");
  });

  it("reports an aborted plan distinctly", () => {
    const plan = {
      op: "rename" as const,
      items: [item(), item({ photo_id: 2 })],
      aborted: true,
      will_create_dir: null,
      destructive: false,
    };
    expect(previewHeadline(plan)).toContain("Plan aborted");
  });
});

describe("resultHeadline / flaggedResults", () => {
  it("states how many of how many completed", () => {
    expect(resultHeadline(summary())).toBe("2 of 2 move complete");
    expect(resultHeadline(summary({ succeeded: 1, failed: 1 }))).toBe(
      "1 of 2 move complete",
    );
  });

  it("flags everything that is not done", () => {
    const s = summary({
      succeeded: 1,
      failed: 1,
      items: [
        opItem(),
        opItem({ source: "/a/b/x.jpg", status: "skipped", detail: "file no longer exists" }),
      ],
    });
    const bad = flaggedResults(s);
    expect(bad).toHaveLength(1);
    expect(bad[0].status).toBe("skipped");
  });
});

describe("progressLabel", () => {
  it("prefers live progress, then the summary, then a neutral line", () => {
    const s = summary();
    expect(progressLabel({ total: 5, done: 2, stage: "move", current: null }, s)).toBe(
      "2 / 5 items",
    );
    expect(progressLabel(null, s)).toBe("2 / 2 items");
    expect(progressLabel(null, null)).toBe("working…");
  });
});
