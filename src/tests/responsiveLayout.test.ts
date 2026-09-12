import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const libraryCss = readFileSync(new URL("../styles/library.css", import.meta.url), "utf8");
const tauriConfig = JSON.parse(
  readFileSync(new URL("../../src-tauri/tauri.conf.json", import.meta.url), "utf8"),
) as { app: { windows: Array<{ minWidth?: number }> } };

describe("Library responsive layout", () => {
  it("makes the compact inspector layout reachable at the minimum desktop width", () => {
    const compactLayout = libraryCss.match(
      /@media \(max-width: (\d+)px\) \{\s*\.library-workspace \{ grid-template-columns: minmax\(0, 1fr\); \}/,
    );
    expect(compactLayout).not.toBeNull();
    expect(Number(compactLayout![1])).toBeGreaterThanOrEqual(
      tauriConfig.app.windows[0].minWidth ?? 0,
    );
  });
});
