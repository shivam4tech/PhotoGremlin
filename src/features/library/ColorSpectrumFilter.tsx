import type { CSSProperties } from "react";
import type { FilterCondition } from "@/types/api";
import {
  PALETTE_COLORS,
  selectedPaletteColors,
  setPaletteColors,
  type PaletteColorId,
} from "./filterFields";

interface ColorSpectrumFilterProps {
  draft: FilterCondition[];
  onChange: (conditions: FilterCondition[]) => void;
  disabled?: boolean;
}

export function ColorSpectrumFilter({ draft, onChange, disabled }: ColorSpectrumFilterProps) {
  const selected = selectedPaletteColors(draft);
  const selectedSet = new Set(selected);

  function toggle(color: PaletteColorId) {
    const next = selectedSet.has(color)
      ? selected.filter((candidate) => candidate !== color)
      : [...selected, color];
    onChange(setPaletteColors(draft, next));
  }

  return (
    <section className="color-spectrum-filter" aria-labelledby="color-spectrum-heading">
      <div className="color-spectrum-head">
        <span>
          <strong id="color-spectrum-heading">Color explorer</strong>
          <small>Select one or more hues found in a photograph</small>
        </span>
        <button
          type="button"
          onClick={() => onChange(setPaletteColors(draft, []))}
          disabled={disabled || selected.length === 0}
        >
          Clear
        </button>
      </div>

      <div className="color-spectrum-layout">
        <div className="color-spectrum-wheel" role="group" aria-label="Colors found in frame">
          <span className="color-spectrum-ring" aria-hidden="true" />
          {PALETTE_COLORS.map((color, index) => {
            const angle = index / PALETTE_COLORS.length * Math.PI * 2 - Math.PI / 2;
            const active = selectedSet.has(color.id);
            const style = {
              "--spectrum-x": `${50 + Math.cos(angle) * 40}%`,
              "--spectrum-y": `${50 + Math.sin(angle) * 40}%`,
            } as CSSProperties;
            return (
              <button
                key={color.id}
                type="button"
                className={`spectrum-choice spectrum-${color.id}${active ? " is-active" : ""}`}
                style={style}
                aria-label={`${color.label}${active ? ", selected" : ""}`}
                aria-pressed={active}
                disabled={disabled}
                onClick={() => toggle(color.id)}
              >
                <span aria-hidden="true" />
              </button>
            );
          })}
          <output className="color-spectrum-count" aria-live="polite">
            <strong>{selected.length || "All"}</strong>
            <span>{selected.length === 1 ? "hue" : selected.length > 1 ? "hues" : "colors"}</span>
          </output>
        </div>

        <div className="color-spectrum-summary">
          {selected.length > 0 ? (
            <>
              <span>Match any selected hue</span>
              <div aria-label="Selected colors">
                {PALETTE_COLORS.filter((color) => selectedSet.has(color.id)).map((color) => (
                  <button
                    key={color.id}
                    type="button"
                    className={`spectrum-${color.id}`}
                    onClick={() => toggle(color.id)}
                    disabled={disabled}
                    aria-label={`Remove ${color.label}`}
                  >
                    <i aria-hidden="true" />{color.label}<b aria-hidden="true">×</b>
                  </button>
                ))}
              </div>
            </>
          ) : (
            <p>All colors are visible. Choose a hue to narrow the library.</p>
          )}
        </div>
      </div>
      <p className="color-spectrum-note">Color signatures are measured locally during photo analysis.</p>
    </section>
  );
}
