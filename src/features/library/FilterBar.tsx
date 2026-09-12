import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { api } from "@/lib/ipc";
import type { FilterCondition, FilterValueOptions } from "@/types/api";
import {
  FILTER_FIELDS,
  FIELD_BY_NAME,
  QUICK_FILTER_PRESETS,
  QUICK_RANGE_FIELDS,
  buildCondition,
  getFilterPresentation,
  isQuickFilterPresetActive,
  toggleQuickFilterPreset,
} from "./filterFields";
import { QuickFilterControls } from "./QuickFilterControls";
import { ColorSpectrumFilter } from "./ColorSpectrumFilter";
import { FilterPicker } from "./FilterPicker";
import { ActiveFilterList } from "./ActiveFilterList";
import { FILTER_CHOICES } from "./filterDiscovery";
import { QuickFilterIcon } from "./QuickFilterIcon";

interface FilterBarProps {
  draft: FilterCondition[];
  onChange: (conditions: FilterCondition[]) => void;
  disabled?: boolean;
  /** Keep EXIF value suggestions relevant to the project/shoot on screen. */
  sessionId?: number | null;
  /** Inspector mode is persistently open inside the Library's right rail. */
  mode?: "bar" | "inspector";
  /** A category limits presentation only; the complete draft is preserved. */
  category?: string;
  onAdvanced?: () => void;
}

const METADATA_VALUE_FIELDS = new Set(["camera_make", "camera_model", "lens"]);
const BESPOKE_FILTER_FIELDS = new Set<string>([...QUICK_RANGE_FIELDS, "palette_color"]);
export const ADVANCED_FILTER_FIELDS = FILTER_FIELDS.filter((definition) => !BESPOKE_FILTER_FIELDS.has(definition.field));
const UNIDENTIFIED = "__photogremlin_unidentified__";

function monthFromDate(value: string): Date {
  const matched = /^(\d{4})-(\d{2})-\d{2}$/.exec(value);
  if (matched) return new Date(Date.UTC(Number(matched[1]), Number(matched[2]) - 1, 1));
  const today = new Date();
  return new Date(Date.UTC(today.getFullYear(), today.getMonth(), 1));
}

function isoDate(year: number, month: number, day: number): string {
  return `${year}-${String(month + 1).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
}

/** Click-only date calendar. Native WebKit date popovers lose selections on
 * some Linux compositors, so this keeps selection in the React filter state. */
function CalendarInput({ label, value, onChange }: { label: string; value: string; onChange: (value: string) => void }) {
  const root = useRef<HTMLSpanElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [month, setMonth] = useState(() => monthFromDate(value));

  useEffect(() => {
    if (value) setMonth(monthFromDate(value));
  }, [value]);

  useEffect(() => {
    if (!open) return;
    function closeOnOutsideClick(event: MouseEvent) {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    }
    window.addEventListener("mousedown", closeOnOutsideClick);
    return () => window.removeEventListener("mousedown", closeOnOutsideClick);
  }, [open]);

  const year = month.getUTCFullYear();
  const monthIndex = month.getUTCMonth();
  const monthName = month.toLocaleString(undefined, { month: "long", timeZone: "UTC" });
  const firstWeekday = new Date(Date.UTC(year, monthIndex, 1)).getUTCDay();
  const days = new Date(Date.UTC(year, monthIndex + 1, 0)).getUTCDate();
  const cells = Array.from({ length: firstWeekday + days }, (_, index) => index < firstWeekday ? null : index - firstWeekday + 1);

  function moveMonth(delta: number) {
    setMonth(new Date(Date.UTC(year, monthIndex + delta, 1)));
  }

  return (
    <span className="date-picker" ref={root} onBlur={(event) => {
      if (!event.currentTarget.contains(event.relatedTarget)) setOpen(false);
    }} onKeyDown={(event) => {
      if (event.key === "Escape" && open) {
        event.stopPropagation(); setOpen(false); trigger.current?.focus();
      }
    }}>
      <button
        ref={trigger}
        className={`date-picker-trigger${value ? " has-value" : ""}`}
        type="button"
        onClick={() => setOpen((wasOpen) => !wasOpen)}
        aria-label={`Select ${label}`}
        aria-expanded={open}
      >
        {value || "Select date"}
        <span aria-hidden="true">▾</span>
      </button>
      {open && (
        <div className="date-picker-popover" role="dialog" aria-label={`Calendar for ${label}`}>
          <div className="date-picker-head">
            <button type="button" onClick={() => moveMonth(-1)} aria-label="Previous month">‹</button>
            <strong>{monthName} {year}</strong>
            <button type="button" onClick={() => moveMonth(1)} aria-label="Next month">›</button>
          </div>
          <div className="date-picker-weekdays">{["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"].map((day) => <span key={day}>{day}</span>)}</div>
          <div className="date-picker-days">
            {cells.map((day, index) => day === null ? <span key={`blank-${index}`} /> : (
              <button
                key={day}
                type="button"
                className={value === isoDate(year, monthIndex, day) ? "is-selected" : ""}
                onClick={() => { onChange(isoDate(year, monthIndex, day)); setOpen(false); trigger.current?.focus(); }}
              >
                {day}
              </button>
            ))}
          </div>
          <div className="date-picker-foot">
            <button type="button" onClick={() => { onChange(""); setOpen(false); trigger.current?.focus(); }} disabled={!value}>Clear</button>
            <button type="button" onClick={() => { const today = new Date(); onChange(isoDate(today.getFullYear(), today.getMonth(), today.getDate())); setOpen(false); trigger.current?.focus(); }}>Today</button>
          </div>
        </div>
      )}
    </span>
  );
}

function ComposerControl({
  label,
  children,
  select = false,
  wide = false,
  className = "",
}: {
  label: string;
  children: ReactNode;
  select?: boolean;
  wide?: boolean;
  className?: string;
}) {
  return (
    <div className={`filter-compose-control${wide ? " is-wide" : ""}${className ? ` ${className}` : ""}`}>
      <span className="filter-compose-label">{label}</span>
      <span className={`filter-compose-input${select ? " is-select" : ""}`}>
        {children}
        {select && <span className="filter-compose-chevron" aria-hidden="true">⌄</span>}
      </span>
    </div>
  );
}

/**
 * The library's active filter, edited as structured conditions (not UI
 * state): what's rendered here is exactly the object sent to the Rust
 * engine and (later) stored in saved views. Neutral technical language
 * throughout (FILTER_ENGINE.md).
 */
export function FilterBar({ draft, onChange, disabled, sessionId = null, mode = "bar", category, onAdvanced }: FilterBarProps) {
  const headingId = useId();
  const root = useRef<HTMLDivElement>(null);
  const composer = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(draft.length > 0);
  // Disclosure controls only editor visibility. Candidate configuration is
  // derived below, while `draft` remains the sole active-filter state.
  const [editorDisclosed, setEditorDisclosed] = useState(false);
  const [field, setField] = useState(ADVANCED_FILTER_FIELDS[0].field);
  const def = FIELD_BY_NAME[field];
  const presentation = getFilterPresentation(field)!;
  const ops = presentation.operatorOptions;
  const [op, setOp] = useState<FilterCondition["operator"]>(ops[0].op);
  const [raw, setRaw] = useState("");
  const [raw2, setRaw2] = useState("");
  const [metadataOptions, setMetadataOptions] = useState<FilterValueOptions | null>(null);
  const [metadataOptionsLoading, setMetadataOptionsLoading] = useState(false);
  const hasMetadataOptions = METADATA_VALUE_FIELDS.has(field);

  useEffect(() => {
    if (editorDisclosed) composer.current?.querySelector<HTMLSelectElement>("select")?.focus();
  }, [editorDisclosed, field]);

  function closeEditor() {
    setEditorDisclosed(false);
    root.current?.querySelector<HTMLInputElement>(".filter-search")?.focus();
  }

  // Keep the operator valid when the field's kind changes.
  useEffect(() => {
    const allowed = getFilterPresentation(field)!.operatorOptions;
    if (!allowed.some((o) => o.op === op)) setOp(allowed[0].op);
  }, [field, op]);

  useEffect(() => {
    if (!hasMetadataOptions) {
      setMetadataOptions(null);
      return;
    }
    let cancelled = false;
    setMetadataOptions(null);
    setMetadataOptionsLoading(true);
    api.filterValueOptions(field as "camera_make" | "camera_model" | "lens", sessionId)
      .then((options) => { if (!cancelled) setMetadataOptions(options); })
      .catch(() => { if (!cancelled) setMetadataOptions(null); })
      .finally(() => { if (!cancelled) setMetadataOptionsLoading(false); });
    return () => { cancelled = true; };
  }, [field, sessionId, hasMetadataOptions]);

  const needsTwoValues = op === "between" && def.kind !== "datetime";
  const needsValue = !["is-null", "not-null"].includes(op);
  const valueUsesSelect = needsValue && (presentation.controlType === "boolean"
    || presentation.controlType === "enum" || hasMetadataOptions);
  const valueUsesPair = needsValue && op === "between";
  const configuredRaw = presentation.controlType === "boolean" && raw === ""
    ? String(presentation.defaultValue)
    : raw;
  const candidate = configuredRaw === UNIDENTIFIED
    ? { field, operator: "is-null" as const, value: null }
    : buildCondition(field, op, configuredRaw, raw2);
  const editorConfigured = candidate !== null;
  const expanded = mode === "inspector" || open;
  const categoryFields = FILTER_CHOICES.filter((choice) => !choice.preset && choice.category === category && choice.field !== "palette_color");
  const rangeFields = categoryFields.map((choice) => choice.field).filter((name) => QUICK_RANGE_FIELDS.some((field) => field === name));
  const rating = draft.find((condition) => condition.field === "rating");
  const ratingThreshold = rating?.operator === ">=" && typeof rating.value === "number" ? rating.value : null;
  const unratedOnly = rating?.operator === "=" && rating.value === 0;

  function setRatingFilter(value: "any" | "unrated" | number) {
    const withoutRating = draft.filter((condition) => condition.field !== "rating");
    if (value === "any") onChange(withoutRating);
    else if (value === "unrated") onChange([...withoutRating, { field: "rating", operator: "=", value: 0 }]);
    else onChange([...withoutRating, { field: "rating", operator: ">=", value }]);
  }

  function selectField(f: string) {
    setEditorDisclosed(true);
    setField(f);
    const first = getFilterPresentation(f)!.operatorOptions[0].op;
    setOp(first);
    setRaw("");
    setRaw2("");
  }

  function add() {
    if (!candidate) return;
    onChange([...draft, candidate]);
    closeEditor();
    setRaw("");
    setRaw2("");
  }

  function valueInput() {
    if (!needsValue) return null;
    switch (def.kind) {
      case "palette":
        return null;
      case "bool":
        return (
          <select
            className="input"
            value={raw === "false" ? "false" : "true"}
            onChange={(e) => setRaw(e.target.value)}
            aria-label={`${def.label} value`}
          >
            {presentation.valueOptions?.map((option) => (
              <option key={String(option.value)} value={String(option.value)}>{option.label}</option>
            ))}
          </select>
        );
      case "text":
        if (hasMetadataOptions) {
          return (
            <select
              className="input"
              value={raw}
              onChange={(e) => {
                const value = e.target.value;
                setRaw(value);
                if (value === UNIDENTIFIED) setOp("is-null");
                else if (op === "is-null" || op === "not-null") setOp("=");
              }}
              disabled={metadataOptionsLoading}
              aria-label={`${def.label} value`}
            >
              <option value="">{metadataOptionsLoading ? "Loading values…" : "— Select from this shoot —"}</option>
              {metadataOptions?.values.map((option) => (
                <option key={option.value} value={option.value}>{option.value} ({option.count.toLocaleString()})</option>
              ))}
              {(metadataOptions?.unidentified_count ?? 0) > 0 && (
                <option value={UNIDENTIFIED}>Unidentified ({metadataOptions!.unidentified_count.toLocaleString()})</option>
              )}
            </select>
          );
        }
        if (def.values) {
          return (
            <select
              className="input"
              value={raw}
              onChange={(e) => setRaw(e.target.value)}
              aria-label={`${def.label} value`}
            >
              <option value="">—</option>
              {presentation.valueOptions?.map((option) => (
                <option key={String(option.value)} value={String(option.value)}>
                  {option.label}
                </option>
              ))}
            </select>
          );
        }
        return (
          <input
            className="input"
            type="text"
            placeholder={op === "in" ? "comma-separated values" : "value"}
            value={raw}
            onChange={(e) => setRaw(e.target.value)}
            aria-label={`${def.label} value`}
          />
        );
      case "datetime":
        if (op === "between") {
          return (
            <>
              <CalendarInput label="from date" value={raw} onChange={setRaw} />
              <span className="faint">→</span>
              <CalendarInput label="to date" value={raw2} onChange={setRaw2} />
            </>
          );
        }
        return (
          <CalendarInput label="capture date" value={raw} onChange={setRaw} />
        );
      case "real":
      case "int":
        if (op === "between") {
          return (
            <>
              <input
                className="input"
                type="number"
                step={def.kind === "int" ? 1 : "any"}
                aria-label={`${def.label} minimum`}
                value={raw}
                onChange={(e) => setRaw(e.target.value)}
              />
              <span className="faint">→</span>
              <input
                className="input"
                type="number"
                step={def.kind === "int" ? 1 : "any"}
                aria-label={`${def.label} maximum`}
                value={raw2}
                onChange={(e) => setRaw2(e.target.value)}
              />
            </>
          );
        }
        if (op === "in") {
          return (
            <input
              className="input"
              type="text"
              placeholder="comma-separated values"
              value={raw}
              onChange={(e) => setRaw(e.target.value)}
              aria-label={`${def.label} value`}
            />
          );
        }
        return (
          <input
            className="input"
            type="number"
            step={def.kind === "int" ? 1 : "any"}
            value={raw}
            onChange={(e) => setRaw(e.target.value)}
            aria-label={`${def.label} value`}
          />
        );
    }
  }

  return (
    <div className={`filterbar filterbar-${mode}`} ref={root}>
      {mode === "bar" && (
        <button
          className={`btn btn-sm ${draft.length > 0 ? "btn-primary" : ""}`}
          onClick={() => setOpen(!open)}
          disabled={disabled}
          aria-expanded={open}
        >
          Filters{draft.length > 0 ? ` (${draft.length})` : ""}
          <span className="faint" style={{ marginLeft: 6 }}>
            {open ? "▲" : "▼"}
          </span>
        </button>
      )}

      {expanded && (
        <div className="filterbar-panel">
          <div className="filter-discovery">
            <FilterPicker draft={draft} disabled={disabled} onSelect={(choice) => {
              if (choice.preset) onChange(toggleQuickFilterPreset(draft, choice.preset));
              else if (choice.field === "palette_color") {
                (root.current?.closest("dialog") ?? root.current)?.querySelector<HTMLButtonElement>(".color-swatch")?.focus();
              } else selectField(choice.field);
            }} />
            {!category && <ActiveFilterList draft={draft} onChange={onChange} disabled={disabled} />}
          </div>
          {!category && <ColorSpectrumFilter draft={draft} onChange={onChange} disabled={disabled} />}

          {(!category || category === "Quick filters") && <section className="quick-presets" aria-labelledby={`${headingId}-quick`}>
            <div className="quick-presets-head">
              <strong id={`${headingId}-quick`}>Quick filters</strong>
              {onAdvanced && <button type="button" className="btn btn-ghost btn-sm" onClick={onAdvanced}>See all</button>}
            </div>
            <div className="quick-presets-list">
              {QUICK_FILTER_PRESETS.map((preset) => {
                const isActive = isQuickFilterPresetActive(draft, preset);
                return (
                  <button
                    key={preset.id}
                    type="button"
                    className={isActive ? "is-active" : ""}
                    aria-pressed={isActive}
                    disabled={disabled}
                    onClick={() => onChange(toggleQuickFilterPreset(draft, preset))}
                  >
                    {category && <QuickFilterIcon id={preset.id} />}
                    <span className="quick-preset-check" aria-hidden="true">{isActive ? "✓" : ""}</span>
                    <span>{preset.label}</span>
                  </button>
                );
              })}
            </div>
          </section>}

          {(!category || category === "Rating & review") && <section className="rating-filter" aria-labelledby={`${headingId}-rating`}>
            <div className="rating-filter-head">
              <strong id={`${headingId}-rating`}>Rating</strong>
              <button type="button" className={!rating ? "is-active" : ""} disabled={disabled} aria-pressed={!rating} onClick={() => setRatingFilter("any")}>Any</button>
              <button type="button" className={unratedOnly ? "is-active" : ""} disabled={disabled} aria-pressed={unratedOnly} onClick={() => setRatingFilter("unrated")}>Unrated</button>
            </div>
            <div className="rating-filter-stars" aria-label="Minimum rating">
              {[1, 2, 3, 4, 5].map((star) => (
                <button
                  key={star}
                  type="button"
                  className={`marks-star${ratingThreshold !== null && ratingThreshold >= star ? " is-on" : ""}`}
                  disabled={disabled}
                  aria-pressed={ratingThreshold === star}
                  aria-label={`${star} stars or more`}
                  onClick={() => setRatingFilter(ratingThreshold === star ? "any" : star)}
                >★</button>
              ))}
              <span className="faint mono">{ratingThreshold ? `${ratingThreshold}+` : unratedOnly ? "0" : "Any"}</span>
            </div>
          </section>}

          {(!category || rangeFields.length > 0) && <QuickFilterControls
            draft={draft}
            onChange={onChange}
            disabled={disabled}
            sessionId={sessionId}
            fields={category ? rangeFields : undefined}
          />}
          {category && categoryFields.length > 0 && <div className="filter-category-fields">
            <h4 className="filter-section-title">Specific conditions</h4>
            {categoryFields.map((choice) => <button key={choice.id} type="button" disabled={disabled}
              className="filter-category-field" onClick={() => selectField(choice.field)}>
              <span>{choice.label}</span><span>{draft.some((condition) => condition.field === choice.field) ? "Added" : "Any"} <span aria-hidden="true">›</span></span>
            </button>)}
          </div>}
          <div className={`more-filters${editorDisclosed ? " is-open" : ""}`}>
            {editorDisclosed && (
              <div className="more-filters-panel" ref={composer} onKeyDown={(event) => {
                if (event.key === "Escape") { event.stopPropagation(); closeEditor(); }
              }}>
                <div className="filterbar-compose">
                  <div className="filter-editor-heading">
                    <strong>{def.label}</strong>
                    <button type="button" className="btn btn-ghost btn-sm" aria-label="Close filter editor"
                      onClick={closeEditor}>×</button>
                  </div>
                  <ComposerControl label="Condition" select>
                    <select
                      className="input"
                      value={op}
                      onChange={(e) => setOp(e.target.value as FilterCondition["operator"])}
                      aria-label="Filter condition"
                    >
                      {ops.map((o) => (
                        <option key={o.op} value={o.op}>
                          {o.label}
                        </option>
                      ))}
                    </select>
                  </ComposerControl>
                  {needsValue && (
                    <ComposerControl
                      label="Value"
                      select={valueUsesSelect}
                      wide={valueUsesPair}
                      className="filter-compose-control-value"
                    >
                      {valueInput()}
                    </ComposerControl>
                  )}
                  <button type="button" className="btn btn-primary btn-sm filterbar-compose-add" onClick={add} disabled={!editorConfigured || disabled}>
                    Add filter
                  </button>
                </div>
                {needsTwoValues && (
                  <div className="faint mono more-filters-hint">
                    between: two values, min → max
                  </div>
                )}
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
