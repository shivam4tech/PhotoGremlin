import { describe, expect, it } from "vitest";
import { clientErrorReport, escapeGtkMarkupText } from "@/lib/errorReporting";

describe("error reporting", () => {
  it("keeps Error details for the local log", () => {
    const error = new Error("renderer failed");
    const report = clientErrorReport("window-error", error);

    expect(report).toMatchObject({ source: "window-error", message: "renderer failed" });
    expect(report.stack).toContain("renderer failed");
  });

  it("turns unknown rejection values into a safe log message", () => {
    expect(clientErrorReport("unhandled-rejection", null)).toEqual({
      source: "unhandled-rejection",
      message: "Unknown browser error",
      stack: null,
    });
  });

  it("escapes every GTK/Pango markup metacharacter", () => {
    expect(escapeGtkMarkupText(`a < b & c > d "quoted" 'single'`)).toBe(
      "a &lt; b &amp; c &gt; d &quot;quoted&quot; &apos;single&apos;",
    );
  });
});
