import type { FilterCondition } from "@/types/api";
import { chipLabel, PALETTE_COLORS } from "./filterFields";

export function ActiveFilterList({ draft, onChange, disabled, label = "Active filters", onEdit, onRemove }: {
  draft: FilterCondition[];
  onChange: (conditions: FilterCondition[]) => void;
  disabled?: boolean;
  label?: string;
  onEdit?: (index: number) => void;
  onRemove?: (index: number) => void;
}) {
  if (!draft.length) return null;
  return <section className="active-filter-list" aria-label={label}>
    <div className="filter-section-heading">{label} <span>{draft.length}</span></div>
    <div className="active-filter-chips">
      {draft.flatMap((condition, index) => {
        const colors = condition.field === "palette_color" && condition.operator === "in" && Array.isArray(condition.value)
          ? PALETTE_COLORS.filter((color) => (condition.value as unknown[]).includes(color.id)) : [];
        if (colors.length) return colors.map((color) => {
          const removeColor = () => {
            const remaining = (condition.value as unknown[]).filter((value) => value !== color.id);
            onChange(draft.flatMap((item, itemIndex) => itemIndex !== index ? [item]
              : remaining.length ? [{ ...item, value: remaining as string[] }] : []));
          };
          if (!onEdit) return <button key={`${index}-${color.id}`} type="button"
            className={`filter-chip spectrum-${color.id}`} disabled={disabled} aria-label={`Remove ${color.label}`}
            onClick={removeColor}><i aria-hidden="true" />{color.label}<span aria-hidden="true">×</span></button>;
          return <span className="filter-chip-group" key={`${index}-${color.id}`}>
            <button type="button" className={`filter-chip filter-chip-edit spectrum-${color.id}`}
              disabled={disabled} aria-label={`Edit ${color.label} color filter`} onClick={() => onEdit(index)}>
              <i aria-hidden="true" />{color.label}
            </button>
            <button type="button" className="filter-chip-remove" disabled={disabled}
              aria-label={`Remove ${color.label}`} onClick={removeColor}>×</button>
          </span>;
        });
        const label = chipLabel(condition);
        if (!onEdit) return [<button key={`${index}-${condition.field}`} type="button" className="filter-chip"
          disabled={disabled} aria-label={`Remove ${label}`}
          onClick={() => onChange(draft.filter((_, itemIndex) => itemIndex !== index))}>
          {label}<span aria-hidden="true">×</span>
        </button>];
        return [<span className="filter-chip-group" key={`${index}-${condition.field}`}>
          <button type="button" className="filter-chip filter-chip-edit" disabled={disabled}
            aria-label={`Edit ${label}`} onClick={() => onEdit(index)}>{label}</button>
          <button type="button" className="filter-chip-remove" disabled={disabled}
            aria-label={`Remove ${label}`} onClick={() => {
              if (onRemove) onRemove(index);
              else onChange(draft.filter((_, itemIndex) => itemIndex !== index));
            }}>×</button>
        </span>];
      })}
    </div>
  </section>;
}
