import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const css = readFileSync(new URL("../styles/theme.css", import.meta.url), "utf8");
function tokens(selector: string) {
  const block = css.slice(css.indexOf(selector)).split("}")[0];
  return Object.fromEntries([...block.matchAll(/--([\w-]+):\s*(#[\da-f]{6});/gi)]
    .map((match) => [match[1], match[2]]));
}
function luminance(hex: string) {
  const channels = [1, 3, 5].map((start) => parseInt(hex.slice(start, start + 2), 16) / 255)
    .map((value) => value <= .04045 ? value / 12.92 : ((value + .055) / 1.055) ** 2.4);
  return channels[0] * .2126 + channels[1] * .7152 + channels[2] * .0722;
}
function contrast(a: string, b: string) {
  const values = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (values[0] + .05) / (values[1] + .05);
}

describe.each([":root {", ':root[data-theme="light"] {'])("theme contrast: %s", (selector) => {
  const theme = tokens(selector);
  it("keeps primary, secondary, tertiary and accent text at AA on workspace surfaces", () => {
    for (const text of ["text-primary", "text-secondary", "text-tertiary", "accent"]) {
      for (const surface of ["bg-canvas", "bg-sidebar", "bg-surface", "bg-surface-elevated", "bg-hover", "bg-selected"]) {
        expect(contrast(theme[text], theme[surface]), `${text} on ${surface}`).toBeGreaterThanOrEqual(4.5);
      }
    }
  });
  it("keeps primary button labels readable in default and hover states", () => {
    for (const surface of ["accent-strong", "accent-hover"]) {
      expect(contrast(theme["accent-on-strong"], theme[surface])).toBeGreaterThanOrEqual(4.5);
    }
  });
});
