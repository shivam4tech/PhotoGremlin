import { describe, it, expect, vi, afterEach } from "vitest";
import {
  DEFAULT_THEME,
  applyTheme,
  persistTheme,
  readStoredTheme,
} from "@/lib/theme";

describe("theme", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("defaults to dark (the darkroom look)", () => {
    expect(DEFAULT_THEME).toBe("dark");
    vi.stubGlobal("localStorage", {
      getItem: () => null,
      setItem: () => {},
      removeItem: () => {},
    });
    expect(readStoredTheme()).toBe("dark");
  });

  it("round-trips the stored preference", () => {
    let v: string | null = null;
    vi.stubGlobal("localStorage", {
      getItem: () => v,
      setItem: (_k: string, val: string) => {
        v = val;
      },
      removeItem: () => {
        v = null;
      },
    });
    persistTheme("light");
    expect(readStoredTheme()).toBe("light");
    persistTheme("dark");
    expect(readStoredTheme()).toBe("dark");
  });

  it("treats unknown stored values as the default", () => {
    vi.stubGlobal("localStorage", { getItem: () => "solarpunk" });
    expect(readStoredTheme()).toBe("dark");
  });

  it("survives storage being unavailable", () => {
    // Node environment: no localStorage at all.
    expect(readStoredTheme()).toBe("dark");
    expect(() => persistTheme("light")).not.toThrow();
  });

  it("applies the theme to the document when one exists", () => {
    const dataset: Record<string, string> = {};
    vi.stubGlobal("document", { documentElement: { dataset } });
    applyTheme("light");
    expect(dataset.theme).toBe("light");
  });
});
