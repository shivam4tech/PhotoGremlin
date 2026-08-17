/**
 * Pure formatting helpers for the statistics UI (Sprint 6).
 * Honesty rules: a `null` metric renders as "unavailable" — never as zero —
 * and the copy reports measurements only (STATISTICS.md §language
 * discipline: "sharpness 62", never "you improved").
 */

/** 0-100 metric with one decimal; null = honest unavailable. */
export function fmtMetric(v: number | null): string {
  if (v === null) return "unavailable";
  return v.toFixed(1);
}

/** Share 0-100 as a percent with no decimals; null = unavailable. */
export function fmtShare(v: number | null): string {
  if (v === null) return "unavailable";
  return `${Math.round(v)}%`;
}

/** 0-1 ratio (kept_ratio) as a percent; null = unavailable. */
export function fmtRatio(v: number | null): string {
  if (v === null) return "unavailable";
  return `${Math.round(v * 100)}%`;
}

/** Duration in days → human: days with one decimal, or hours when small. */
export function fmtDuration(days: number | null): string {
  if (days === null) return "unknown";
  if (days < 1) {
    const hours = days * 24;
    return hours < 1 ? "< 1 hour" : `${Math.round(hours)} hours`;
  }
  if (days < 10) return `${days.toFixed(1)} days`;
  return `${Math.round(days)} days`;
}

/** ISO average → integer "640"; aperture → "f/5.6"; shutter → "1/125". */
export function fmtIso(v: number | null): string {
  if (v === null) return "unavailable";
  return String(Math.round(v));
}

export function fmtAperture(v: number | null): string {
  if (v === null) return "unavailable";
  return `f/${v.toFixed(1)}`;
}

export function fmtShutter(v: number | null): string {
  if (v === null) return "unavailable";
  if (v >= 1) return v === 1 ? "1s" : `${v.toFixed(1)}s`;
  const denom = Math.round(1 / v);
  const approx = Math.abs(1 / denom - v) / v < 0.05;
  return approx ? `1/${denom}` : `${v.toFixed(3)}s`;
}

/** "2026-06" → "Jun 2026" (fixed month names, no locale dependence). */
const MONTHS = [
  "Jan", "Feb", "Mar", "Apr", "May", "Jun",
  "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
] as const;

export function monthLabel(ym: string): string {
  const [y, m] = ym.split("-").map(Number);
  if (!y || !m || m < 1 || m > 12) return ym;
  return `${MONTHS[m - 1]} ${y}`;
}

/** Max bin count in a histogram (bar scaling); 0 when empty. */
export function maxCount(bins: { count: number }[]): number {
  return bins.reduce((m, b) => Math.max(m, b.count), 0);
}
