import { describe, expect, it } from "vitest";
import {
  buildSystemdArgs,
  formatGib,
  parseMemAvailable,
  resolveTask,
  shouldBlockForMemory,
} from "./run-resource-guarded.mjs";

describe("resource-guarded build launcher", () => {
  it("reads Linux MemAvailable values in bytes", () => {
    expect(parseMemAvailable("MemTotal: 16384 kB\nMemAvailable: 6144 kB\n")).toBe(
      6144 * 1024,
    );
    expect(parseMemAvailable("MemFree: 6144 kB\n")).toBeNull();
  });

  it("blocks low-memory starts unless explicitly overridden", () => {
    const fiveGib = 5 * 1024 ** 3;
    expect(shouldBlockForMemory(fiveGib, undefined)).toBe(true);
    expect(shouldBlockForMemory(fiveGib, "0")).toBe(true);
    expect(shouldBlockForMemory(fiveGib, "1")).toBe(false);
    expect(shouldBlockForMemory(6 * 1024 ** 3, undefined)).toBe(false);
  });

  it("builds a bounded user-systemd scope", () => {
    const args = buildSystemdArgs(resolveTask("rust-test"));
    expect(args).toContain("--property=MemoryHigh=3G");
    expect(args).toContain("--property=MemoryMax=4G");
    expect(args).toContain("--property=MemorySwapMax=1G");
    expect(args.slice(-3)).toEqual(["--", "cargo", "test"]);
  });

  it("rejects unknown task names and formats GiB readings", () => {
    expect(() => resolveTask("unknown")).toThrow("Unknown guarded task");
    expect(formatGib(6 * 1024 ** 3)).toBe("6.0");
  });
});
