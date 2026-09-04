/**
 * Shared curatorial-marks constants (Sprint 13). The color enum must mirror
 * the Rust `COLOR_LABELS` (src-tauri/src/database.rs).
 */
export const MARK_COLORS: { name: string; hex: string }[] = [
  { name: "red", hex: "#cf3e3e" },
  { name: "yellow", hex: "#d9a52c" },
  { name: "green", hex: "#3e9e5c" },
  { name: "blue", hex: "#2f7fd0" },
  { name: "purple", hex: "#8a5fce" },
  { name: "gray", hex: "#8a8f98" },
];

export const LABEL_HEX: Record<string, string> = Object.fromEntries(
  MARK_COLORS.map((c) => [c.name, c.hex]),
);