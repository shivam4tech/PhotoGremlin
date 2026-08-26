/**
 * Filter field registry + pure helpers (Sprint 5 frontend half).
 *
 * Mirrors the Rust `filters` field registry (FILTER_ENGINE.md). Keeping the
 * field/operator knowledge here means the bar, the chips, and future saved
 * views all speak one language, and it stays unit-testable without a DOM.
 */
import type { Filter, FilterCondition } from "@/types/api";

export type FieldKind = "real" | "int" | "text" | "bool" | "datetime";

export interface FieldDef {
  field: string;
  label: string;
  kind: FieldKind;
  area: string;
  /** Fixed value set (orientation). Text fields without this use free input. */
  values?: string[];
}

export const FILTER_FIELDS: FieldDef[] = [
  { field: "sharpness", label: "Sharpness", kind: "real", area: "Technical" },
  { field: "brightness", label: "Brightness", kind: "real", area: "Technical" },
  { field: "contrast", label: "Contrast", kind: "real", area: "Technical" },
  { field: "saturation", label: "Saturation", kind: "real", area: "Technical" },
  { field: "highlight_clipping", label: "Highlight clipping", kind: "real", area: "Technical" },
  { field: "shadow_clipping", label: "Shadow clipping", kind: "real", area: "Technical" },
  { field: "monochrome", label: "Monochrome", kind: "bool", area: "Visual" },
  { field: "color", label: "Color", kind: "bool", area: "Visual" },
  { field: "dark", label: "Dark photo", kind: "bool", area: "Visual" },
  { field: "bright", label: "Bright photo", kind: "bool", area: "Visual" },
  {
    field: "orientation",
    label: "Orientation",
    kind: "text",
    area: "Orientation",
    values: ["landscape", "portrait", "square"],
  },
  { field: "camera_make", label: "Camera make", kind: "text", area: "Camera & lens" },
  { field: "camera_model", label: "Camera model", kind: "text", area: "Camera & lens" },
  { field: "lens", label: "Lens", kind: "text", area: "Camera & lens" },
  { field: "iso", label: "ISO", kind: "int", area: "Exposure" },
  { field: "aperture", label: "Aperture (f-number)", kind: "real", area: "Exposure" },
  { field: "shutter_speed", label: "Shutter speed (s)", kind: "real", area: "Exposure" },
  { field: "focal_length", label: "Focal length (mm)", kind: "real", area: "Exposure" },
  { field: "capture_datetime", label: "Capture date", kind: "datetime", area: "Time" },
  {
    field: "faces_present",
    label: "Contains faces",
    kind: "bool",
    area: "Faces & smiles (local models)",
  },
  { field: "smiling", label: "Smiling", kind: "bool", area: "Faces & smiles (local models)" },
  {
    field: "scene_group",
    label: "Scene group",
    kind: "text",
    area: "Scene (local model)",
  },
  {
    field: "rating",
    label: "Rating",
    kind: "int",
    area: "Marking",
  },
  { field: "flagged", label: "Flagged", kind: "bool", area: "Marking" },
  {
    field: "color_label",
    label: "Color label",
    kind: "text",
    area: "Marking",
    values: ["red", "yellow", "green", "blue", "purple", "gray"],
  },
];

export const FIELD_BY_NAME: Record<string, FieldDef> = Object.fromEntries(
  FILTER_FIELDS.map((f) => [f.field, f]),
);

export const AREA_ORDER: string[] = [...new Set(FILTER_FIELDS.map((f) => f.area))];

export interface OpDef {
  op: FilterCondition["operator"];
  label: string;
}

const RELATIONAL: OpDef[] = [
  { op: "=", label: "=" },
  { op: "!=", label: "≠" },
  { op: ">", label: ">" },
  { op: ">=", label: "≥" },
  { op: "<", label: "<" },
  { op: "<=", label: "≤" },
];

export const OPS_BY_KIND: Record<FieldKind, OpDef[]> = {
  real: [
    ...RELATIONAL,
    { op: "between", label: "between" },
    { op: "in", label: "in" },
    { op: "is-null", label: "not recorded" },
    { op: "not-null", label: "recorded" },
  ],
  int: [
    ...RELATIONAL,
    { op: "between", label: "between" },
    { op: "in", label: "in" },
    { op: "is-null", label: "not recorded" },
    { op: "not-null", label: "recorded" },
  ],
  datetime: [
    { op: "between", label: "between" },
    { op: ">=", label: "on/after" },
    { op: "<=", label: "on/before" },
    { op: ">", label: "after" },
    { op: "<", label: "before" },
    { op: "is-null", label: "not recorded" },
    { op: "not-null", label: "recorded" },
  ],
  bool: [{ op: "=", label: "is" }, { op: "!=", label: "is not" }],
  text: [
    { op: "=", label: "=" },
    { op: "!=", label: "≠" },
    { op: "in", label: "in" },
    { op: "is-null", label: "not recorded" },
    { op: "not-null", label: "recorded" },
  ],
};

const BOOL_PHRASES: Record<string, string> = {
  monochrome: "monochrome",
  color: "in color",
  dark: "dark",
  bright: "bright",
  faces_present: "contains faces",
  smiling: "smiling",
  flagged: "flagged",
};

const OP_SYMBOL: Record<string, string> = {
  "=": "=",
  "!=": "≠",
  ">": ">",
  ">=": "≥",
  "<": "<",
  "<=": "≤",
};

/** Bare date ("YYYY-MM-DD") → end-of-day UTC instant for `between` bounds.
 * Full timestamps pass through unchanged. */
export function endOfDay(dateStr: string): string {
  return /^\d{4}-\d{2}-\d{2}$/.test(dateStr) ? `${dateStr}T23:59:59Z` : dateStr;
}

/**
 * Compose one condition the way the bar does. Returns null when the raw
 * inputs are not yet valid (the panel just doesn't enable "Add").
 */
export function buildCondition(
  field: string,
  op: FilterCondition["operator"],
  raw: string,
  raw2: string,
): FilterCondition | null {
  const def = FIELD_BY_NAME[field];
  if (!def) return null;
  const num = (s: string): number | null =>
    s.trim().length === 0 ? null : Number(s.trim());
  switch (def.kind) {
    case "real": {
      if (op === "between") {
        const lo = num(raw);
        const hi = num(raw2);
        if (lo === null || hi === null || !Number.isFinite(lo) || !Number.isFinite(hi)) {
          return null;
        }
        return { field, operator: op, value: [lo, hi] };
      }
      if (op === "in") {
        const items = raw
          .split(",")
          .map((s) => s.trim())
          .filter((s) => s.length > 0)
          .map((s) => Number(s))
          .filter((n) => Number.isFinite(n));
        if (items.length === 0) return null;
        return { field, operator: op, value: items };
      }
      if (op === "is-null" || op === "not-null") {
        return { field, operator: op, value: null };
      }
      const v = num(raw);
      return v !== null && Number.isFinite(v) ? { field, operator: op, value: v } : null;
    }
    case "int": {
      if (op === "between") {
        const lo = num(raw);
        const hi = num(raw2);
        if (lo === null || hi === null || !Number.isInteger(lo) || !Number.isInteger(hi)) {
          return null;
        }
        return { field, operator: op, value: [lo, hi] };
      }
      if (op === "in") {
        const items = raw
          .split(",")
          .map((s) => s.trim())
          .filter((s) => s.length > 0)
          .map((s) => Number(s))
          .filter((n) => Number.isInteger(n));
        if (items.length === 0) return null;
        return { field, operator: op, value: items };
      }
      if (op === "is-null" || op === "not-null") {
        return { field, operator: op, value: null };
      }
      const v = num(raw);
      return v !== null && Number.isInteger(v) ? { field, operator: op, value: v } : null;
    }
    case "text": {
      if (op === "in") {
        const items = raw
          .split(",")
          .map((s) => s.trim())
          .filter((s) => s.length > 0);
        return items.length > 0 ? { field, operator: op, value: items } : null;
      }
      if (op === "is-null" || op === "not-null") {
        return { field, operator: op, value: null };
      }
      const v = raw.trim();
      if (v.length === 0) return null;
      if (def.values && !def.values.includes(v)) return null;
      return { field, operator: op, value: v };
    }
    case "bool": {
      // The bar's "is" select stores the boolean directly.
      const b = raw === "true";
      void raw2;
      return { field, operator: op, value: b };
    }
    case "datetime": {
      if (op === "is-null" || op === "not-null") {
        return { field, operator: op, value: null };
      }
      if (op === "between") {
        if (!raw || !raw2) return null;
        return { field, operator: op, value: [raw, endOfDay(raw2)] };
      }
      if (!raw) return null;
      return { field, operator: op, value: raw };
    }
  }
}

/** Neutral, technical chip text (FILTER_ENGINE.md language rules). */
export function chipLabel(c: FilterCondition): string {
  const def = FIELD_BY_NAME[c.field];
  const label = def ? def.label : c.field;
  if (c.operator === "is-null") return `${label.toLowerCase()}: not recorded`;
  if (c.operator === "not-null") return `${label.toLowerCase()}: recorded`;
  if (c.operator === "in") {
    const items = Array.isArray(c.value) ? (c.value as unknown[]).join(", ") : "";
    return `${label.toLowerCase()} in {${items}}`;
  }
  if (c.operator === "between" && Array.isArray(c.value) && c.value.length === 2) {
    const [lo, hi] = c.value as [unknown, unknown];
    const hiTxt =
      typeof hi === "string" && hi.endsWith("T23:59:59Z") ? hi.slice(0, 10) : String(hi);
    return `${label.toLowerCase()} ${lo} → ${hiTxt}`;
  }
  if (def?.kind === "bool") {
    const phrase = BOOL_PHRASES[c.field] ?? label.toLowerCase();
    return c.operator === "!=" ? `not ${phrase}` : phrase;
  }
  return `${label.toLowerCase()} ${OP_SYMBOL[c.operator] ?? c.operator} ${String(c.value)}`;
}

export function draftToFilter(conditions: FilterCondition[]): Filter {
  return { operator: "AND", conditions };
}

export function filterToDraft(filter: Filter): FilterCondition[] {
  return filter.conditions;
}

