import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const themeCss = readFileSync(new URL("../styles/theme.css", import.meta.url), "utf8");

describe("Photo tile presentation", () => {
  it("preserves the complete photograph instead of cropping it to the tile", () => {
    const tileImageRule = themeCss.match(/\.tile-img\s*\{([^}]*)\}/)?.[1] ?? "";

    expect(tileImageRule).toContain("object-fit: contain");
    expect(tileImageRule).toContain("object-position: center");
    expect(tileImageRule).not.toContain("object-fit: cover");
  });
});
