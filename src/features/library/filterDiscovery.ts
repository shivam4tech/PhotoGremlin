import type { Filter, FilterCondition } from "@/types/api";
import { FILTER_FIELDS, QUICK_FILTER_PRESETS, isQuickFilterPresetActive } from "./filterFields";

/** Presentation only: conditions always use the existing field registry. */
export const FILTER_CHOICES = [
  ...QUICK_FILTER_PRESETS.map((preset) => ({
    id: `quick-${preset.id}`, label: preset.label, category: "Quick filters",
    field: preset.condition.field, preset,
  })),
  ...FILTER_FIELDS.map((field) => ({
    id: field.field, label: field.label,
    category: field.area.includes("local") || field.area === "Burst context" ? "People & content"
      : ["Camera & lens", "Exposure", "Time"].includes(field.area) ? "Camera & date"
      : ["Marking", "Review"].includes(field.area) ? "Rating & review"
      : field.area === "Technical" ? "Image properties" : "Appearance",
    field: field.field, preset: undefined,
  })),
];
export type FilterChoice = (typeof FILTER_CHOICES)[number];

export function searchFilterChoices(query: string): FilterChoice[] {
  const words = query.toLowerCase().trim().split(/\s+/).filter(Boolean);
  return FILTER_CHOICES.filter((choice) => {
    const people = /face|eye|blink|smil/.test(choice.field) ? "face eyes people" : "";
    const text = `${choice.label} ${choice.field.replaceAll("_", " ")} ${choice.category} ${people}`.toLowerCase();
    return words.every((word) => text.includes(word));
  }).sort((a, b) => a.category === b.category ? 0 : a.category === "Quick filters" ? -1 : b.category === "Quick filters" ? 1 : a.category.localeCompare(b.category));
}

export function choiceIsActive(choice: FilterChoice, draft: FilterCondition[]): boolean {
  return choice.preset ? isQuickFilterPresetActive(draft, choice.preset)
    : draft.some((condition) => condition.field === choice.field);
}

/** An indicator only, never a replacement for backend saved-view validation. */
export function savedViewMatches(filterJson: string, draft: FilterCondition[]): boolean {
  try {
    const filter = JSON.parse(filterJson) as Filter;
    if (filter.operator !== "AND" || !Array.isArray(filter.conditions)) return false;
    const key = (items: FilterCondition[]) => items.map(({ field, operator, value }) =>
      JSON.stringify([field, operator, operator === "in" && Array.isArray(value) ? [...value].sort() : value])).sort().join("\n");
    return key(filter.conditions) === key(draft);
  } catch { return false; }
}
