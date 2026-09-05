import type { FilterCondition } from "@/types/api";
import { chipLabel, PALETTE_COLORS } from "./filterFields";

export function ActiveFilterList({ draft, onChange, disabled }: {
  draft: FilterCondition[];
  onChange: (conditions: FilterCondition[]) => void;
  disabled?: boolean;
}) {
  if (!draft.length) return null;
  return <section className="active-filter-list" aria-label="Active filters">
    <div className="filter-section-heading">Active filters <span>{draft.length}</span></div>
    <div className="active-filter-chips">
      {draft.flatMap((condition, index) => {
        const colors = condition.field === "palette_color" && condition.operator === "in" && Array.isArray(condition.value)
          ? PALETTE_COLORS.filter((color) => (condition.value as unknown[]).includes(color.id)) : [];
        if (colors.length) return colors.map((color) => <button key={`${index}-${color.id}`} type="button"
          className={`filter-chip spectrum-${color.id}`} disabled={disabled} aria-label={`Remove ${color.label}`}
          onClick={() => {
            const remaining = (condition.value as unknown[]).filter((value) => value !== color.id);
            onChange(draft.flatMap((item, itemIndex) => itemIndex !== index ? [item]
              : remaining.length ? [{ ...item, value: remaining as string[] }] : []));
          }}><i aria-hidden="true" />{color.label}<span aria-hidden="true">×</span></button>);
        const label = chipLabel(condition);
        return [<button key={`${index}-${condition.field}`} type="button" className="filter-chip"
          disabled={disabled} aria-label={`Remove ${label}`}
          onClick={() => onChange(draft.filter((_, itemIndex) => itemIndex !== index))}>
          {label}<span aria-hidden="true">×</span>
        </button>];
      })}
    </div>
  </section>;
}
