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
  { field: "eye_closure_confidence", label: "Eye closure confidence", kind: "real", area: "Faces & eyes (local models)" },
  { field: "closed_eye_candidate", label: "Closed-eye candidate", kind: "bool", area: "Faces & eyes (local models)" },
  { field: "possible_blink", label: "Possible blink", kind: "bool", area: "Burst context" },
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
    field: "scene_fine",
    label: "Scene label",
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
    field: "review_state",
    label: "Review state",
    kind: "text",
    area: "Review",
    values: ["selected", "rejected", "needs_attention"],
  },
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

export type QuickRangeField =
  | "brightness"
  | "sharpness"
  | "contrast"
  | "highlight_clipping"
  | "shadow_clipping"
  | "eye_closure_confidence"
  | "iso"
  | "focal_length";

export const QUICK_RANGE_FIELDS: readonly QuickRangeField[] = [
  "brightness",
  "sharpness",
  "contrast",
  "highlight_clipping",
  "shadow_clipping",
  "eye_closure_confidence",
  "iso",
  "focal_length",
];

export const STANDARD_FILTER_STOPS = {
  iso: [25, 50, 100, 200, 400, 800, 1600, 3200, 6400, 12800, 25600, 51200, 102400],
  focal_length: [8, 14, 16, 20, 24, 28, 35, 50, 70, 85, 105, 135, 200, 300, 400, 600, 800, 1200],
} as const;

export interface QuickRangeBounds {
  lower: number;
  upper: number;
  missingOnly: boolean;
  editable: boolean;
}

export interface QuickFilterPreset {
  id: string;
  label: string;
  condition: FilterCondition;
  /** Fields removed together so paired shortcuts cannot contradict each other. */
  exclusiveFields: readonly string[];
}

export const QUICK_FILTER_PRESETS: readonly QuickFilterPreset[] = [
  {
    id: "potentially-soft",
    label: "Potentially soft",
    condition: { field: "sharpness", operator: "<", value: 40 },
    exclusiveFields: ["sharpness"],
  },
  {
    id: "highlight-clipping",
    label: "Highlight clipping",
    condition: { field: "highlight_clipping", operator: ">=", value: 5 },
    exclusiveFields: ["highlight_clipping"],
  },
  {
    id: "shadow-clipping",
    label: "Shadow clipping",
    condition: { field: "shadow_clipping", operator: ">=", value: 5 },
    exclusiveFields: ["shadow_clipping"],
  },
  {
    id: "closed-eye-candidate",
    label: "Closed-eye candidate",
    condition: { field: "closed_eye_candidate", operator: "=", value: true },
    exclusiveFields: ["closed_eye_candidate"],
  },
  {
    id: "possible-blink",
    label: "Possible blink",
    condition: { field: "possible_blink", operator: "=", value: true },
    exclusiveFields: ["possible_blink"],
  },
  {
    id: "monochrome",
    label: "Black & white",
    condition: { field: "monochrome", operator: "=", value: true },
    exclusiveFields: ["monochrome", "color"],
  },
  {
    id: "color",
    label: "Color",
    condition: { field: "monochrome", operator: "=", value: false },
    exclusiveFields: ["monochrome", "color"],
  },
  {
    id: "dark",
    label: "Dark",
    condition: { field: "dark", operator: "=", value: true },
    exclusiveFields: ["dark", "bright"],
  },
  {
    id: "bright",
    label: "Bright",
    condition: { field: "bright", operator: "=", value: true },
    exclusiveFields: ["dark", "bright"],
  },
  {
    id: "landscape",
    label: "Landscape",
    condition: { field: "orientation", operator: "=", value: "landscape" },
    exclusiveFields: ["orientation"],
  },
  {
    id: "portrait",
    label: "Portrait",
    condition: { field: "orientation", operator: "=", value: "portrait" },
    exclusiveFields: ["orientation"],
  },
  {
    id: "faces",
    label: "Contains faces",
    condition: { field: "faces_present", operator: "=", value: true },
    exclusiveFields: ["faces_present"],
  },
];

export function isQuickFilterPresetActive(
  conditions: FilterCondition[],
  preset: QuickFilterPreset,
): boolean {
  return conditions.some((condition) => condition.field === preset.condition.field
    && condition.operator === preset.condition.operator
    && JSON.stringify(condition.value) === JSON.stringify(preset.condition.value));
}

/** Toggle a preset while clearing only its mutually-exclusive shortcut group. */
export function toggleQuickFilterPreset(
  conditions: FilterCondition[],
  preset: QuickFilterPreset,
): FilterCondition[] {
  const alreadyActive = isQuickFilterPresetActive(conditions, preset);
  const remaining = conditions.filter(
    (condition) => !preset.exclusiveFields.includes(condition.field),
  );
  return alreadyActive ? remaining : [...remaining, preset.condition];
}

/** Convert the inclusive range scrubber into the ordinary filter wire format. */
export function quickRangeCondition(
  field: QuickRangeField,
  lower: number,
  upper: number,
  domainLower: number,
  domainUpper: number,
): FilterCondition | null {
  const hasLower = lower > domainLower;
  const hasUpper = upper < domainUpper;
  if (!hasLower && !hasUpper) return null;
  if (hasLower && hasUpper) return { field, operator: "between", value: [lower, upper] };
  if (hasLower) return { field, operator: ">=", value: lower };
  return { field, operator: "<=", value: upper };
}

/** Read both new range filters and legacy strict quick-filter conditions.
 * Strictness is preserved in the condition itself until the photographer
 * moves a handle, at which point the scrubber emits its inclusive model. */
export function quickRangeBounds(
  condition: FilterCondition | undefined,
  domainLower: number,
  domainUpper: number,
): QuickRangeBounds {
  const full = { lower: domainLower, upper: domainUpper, missingOnly: false, editable: true };
  if (!condition) return full;
  if (condition.operator === "is-null") return { ...full, missingOnly: true };
  if (condition.operator === "not-null") return { ...full, editable: false };
  if (condition.operator === "=" && typeof condition.value === "number") {
    return { lower: condition.value, upper: condition.value, missingOnly: false, editable: true };
  }
  if ([">", ">="].includes(condition.operator) && typeof condition.value === "number") {
    return { ...full, lower: condition.value };
  }
  if (["<", "<="].includes(condition.operator) && typeof condition.value === "number") {
    return { ...full, upper: condition.value };
  }
  if (
    condition.operator === "between"
    && Array.isArray(condition.value)
    && condition.value.length === 2
    && condition.value.every((value) => typeof value === "number")
  ) {
    return {
      lower: condition.value[0] as number,
      upper: condition.value[1] as number,
      missingOnly: false,
      editable: true,
    };
  }
  return { ...full, editable: false };
}

/** A quick control owns its field. Replacing it removes advanced conditions
 * for the same field, preventing contradictory hidden ranges. */
export function replaceFieldConditions(
  conditions: FilterCondition[],
  field: string,
  replacement: FilterCondition | null,
): FilterCondition[] {
  const others = conditions.filter((condition) => condition.field !== field);
  return replacement ? [...others, replacement] : others;
}

/** Toggle one exact condition while preserving every other field. Used by
 * compact workspace shortcuts such as Review views. */
export function toggleExactFieldCondition(
  conditions: FilterCondition[],
  candidate: FilterCondition,
): FilterCondition[] {
  const current = conditions.find((condition) => condition.field === candidate.field);
  const alreadyActive = current?.operator === candidate.operator
    && JSON.stringify(current.value) === JSON.stringify(candidate.value);
  return replaceFieldConditions(conditions, candidate.field, alreadyActive ? null : candidate);
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
