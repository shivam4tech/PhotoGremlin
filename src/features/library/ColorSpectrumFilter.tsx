import type { FilterCondition } from "@/types/api";
import { PALETTE_COLORS, selectedPaletteColors, setPaletteColors, type PaletteColorId } from "./filterFields";

export function ColorSpectrumFilter({ draft, onChange, disabled }: {
  draft: FilterCondition[];
  onChange: (conditions: FilterCondition[]) => void;
  disabled?: boolean;
}) {
  const selected = selectedPaletteColors(draft);
  function toggle(color: PaletteColorId) {
    onChange(setPaletteColors(draft, selected.includes(color)
      ? selected.filter((item) => item !== color) : [...selected, color]));
  }
  return <section className="color-filter" aria-label="Colors in frame">
    <div className="filter-section-heading">Colors in frame</div>
    <div className="color-swatches" role="group" aria-label="Colors found in frame">
      {PALETTE_COLORS.map((color) => <button key={color.id} type="button"
        className={`color-swatch spectrum-${color.id}`} aria-label={color.label}
        title={color.label} aria-pressed={selected.includes(color.id)} disabled={disabled}
        onClick={() => toggle(color.id)}>{selected.includes(color.id) && <span aria-hidden="true">✓</span>}</button>)}
    </div>
    <p className="filter-helper">{selected.length ? "Match any selected hue" : "Choose one or more hues"}</p>
  </section>;
}
