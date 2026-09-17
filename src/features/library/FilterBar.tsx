import { useEffect, useId, useMemo, useRef, useState, type Dispatch, type ReactNode } from "react";
import { api } from "@/lib/ipc";
import type { FilterCondition, FilterValueOptions, NumericFilterStats } from "@/types/api";
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
import { PrecisionRangeFilter, QuickFilterControls, RANGE_SPECS } from "./QuickFilterControls";
import { ColorSpectrumFilter } from "./ColorSpectrumFilter";
import { FilterPicker } from "./FilterPicker";
import { ActiveFilterList } from "./ActiveFilterList";
import { FILTER_CHOICES } from "./filterDiscovery";
import type { AdvancedFilterEditorState, AdvancedFilterWorkspaceAction } from "./advancedFilterState";

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
  /** Advanced mode may own editor transitions alongside its private draft. */
  editorState?: AdvancedFilterEditorState;
  editorDispatch?: Dispatch<AdvancedFilterWorkspaceAction>;
  onPaletteSelect?: () => void;
  autoFocusSearch?: boolean;
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

function conditionInputs(condition: FilterCondition | null): [string, string] {
  if (!condition || condition.value === null || condition.value === undefined) return ["", ""];
  if (condition.operator === "between" && Array.isArray(condition.value)) {
    const upper = String(condition.value[1] ?? "");
    return [String(condition.value[0] ?? ""), upper.endsWith("T23:59:59Z") ? upper.slice(0, 10) : upper];
  }
  if (Array.isArray(condition.value)) return [condition.value.join(", "), ""];
  return [String(condition.value), ""];
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

function OptionPicker({ label, value, options, onChange, disabled = false }: {
  label: string;
  value: string;
  options: readonly { value: string; label: string }[];
  onChange: (value: string) => void;
  disabled?: boolean;
}) {
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState("");
  const [highlighted, setHighlighted] = useState(0);
  const filtered = options.filter((option) => option.label.toLowerCase().includes(search.toLowerCase()));
  useEffect(() => {
    if (!open) return;
    const close = (event: MouseEvent) => { if (!root.current?.contains(event.target as Node)) setOpen(false); };
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, [open]);
  function choose(next: string) {
    onChange(next); setOpen(false); setSearch(""); trigger.current?.focus();
  }
  return <div className="filter-option-picker" ref={root} onKeyDown={(event) => {
    if (event.key === "Escape" && open) { event.stopPropagation(); setOpen(false); trigger.current?.focus(); }
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault(); setOpen(true);
      setHighlighted((current) => Math.max(0, Math.min(filtered.length - 1, current + (event.key === "ArrowDown" ? 1 : -1))));
    }
    if (event.key === "Enter" && open && filtered[highlighted]) { event.preventDefault(); choose(filtered[highlighted].value); }
  }}>
    <button ref={trigger} type="button" className="filter-option-trigger" aria-label={label} aria-expanded={open} disabled={disabled}
      onClick={() => { setOpen((current) => !current); setHighlighted(Math.max(0, options.findIndex((option) => option.value === value))); }}>
      <span>{options.find((option) => option.value === value)?.label ?? "Choose…"}</span><span aria-hidden="true">⌄</span>
    </button>
    {open && <div className="filter-option-popover">
      {options.length > 6 && <input className="input" type="search" value={search} placeholder="Find a value…" aria-label={`Search ${label}`}
        onChange={(event) => { setSearch(event.target.value); setHighlighted(0); }} autoFocus />}
      <div className="filter-option-list" role="listbox" aria-label={label}>{filtered.map((option, index) => <button key={option.value} type="button" role="option"
        aria-selected={option.value === value} className={index === highlighted ? "is-highlighted" : ""}
        onMouseEnter={() => setHighlighted(index)} onClick={() => choose(option.value)}>
        <span>{option.label}</span><span aria-hidden="true">{option.value === value ? "✓" : ""}</span>
      </button>)}</div>
      {filtered.length === 0 && <span className="filter-option-empty">No values found</span>}
    </div>}
  </div>;
}

/**
 * The library's active filter, edited as structured conditions (not UI
 * state): what's rendered here is exactly the object sent to the Rust
 * engine and (later) stored in saved views. Neutral technical language
 * throughout (FILTER_ENGINE.md).
 */
export function FilterBar({
  draft,
  onChange,
  disabled,
  sessionId = null,
  mode = "bar",
  category,
  onAdvanced,
  editorState,
  editorDispatch,
  onPaletteSelect,
  autoFocusSearch = true,
}: FilterBarProps) {
  const headingId = useId();
  const root = useRef<HTMLDivElement>(null);
  const composer = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(draft.length > 0);
  // Disclosure controls only editor visibility. Candidate configuration is
  // derived below, while `draft` remains the sole active-filter state.
  const [localEditorDisclosed, setLocalEditorDisclosed] = useState(false);
  const [field, setField] = useState(ADVANCED_FILTER_FIELDS[0].field);
  const def = FIELD_BY_NAME[field];
  const presentation = getFilterPresentation(field)!;
  const ops = presentation.operatorOptions;
  const [op, setOp] = useState<FilterCondition["operator"]>(ops[0].op);
  const [raw, setRaw] = useState("");
  const [raw2, setRaw2] = useState("");
  const [rangeCandidate, setRangeCandidate] = useState<FilterCondition | null>(null);
  const [rangeStats, setRangeStats] = useState<NumericFilterStats | undefined>();
  const [rangeStatsReady, setRangeStatsReady] = useState(false);
  const [metadataOptions, setMetadataOptions] = useState<FilterValueOptions | null>(null);
  const [metadataOptionsLoading, setMetadataOptionsLoading] = useState(false);
  const hasMetadataOptions = METADATA_VALUE_FIELDS.has(field);
  const controlledEditor = editorState?.kind === "editing" ? editorState : null;
  const editorDisclosed = editorState ? controlledEditor !== null : localEditorDisclosed;

  const editorIdentity = controlledEditor
    ? `${controlledEditor.intent}:${controlledEditor.field}:${controlledEditor.intent === "edit" ? controlledEditor.index : "new"}`
    : "";

  useEffect(() => {
    if (!controlledEditor) return;
    const seed = controlledEditor.candidate;
    const nextField = controlledEditor.field;
    let nextOperator = seed?.operator ?? getFilterPresentation(nextField)!.operatorOptions[0].op;
    let [nextRaw, nextRaw2] = conditionInputs(seed);
    // The engine supports `!=`, but the binary editor can express the same
    // condition more plainly by selecting the opposite named state.
    if (FIELD_BY_NAME[nextField]?.kind === "bool" && nextOperator === "!="
      && (nextRaw === "true" || nextRaw === "false")) {
      nextOperator = "=";
      nextRaw = nextRaw === "true" ? "false" : "true";
    }
    setField(nextField);
    setOp(nextOperator);
    setRaw(nextRaw);
    setRaw2(nextRaw2);
    setRangeCandidate(getFilterPresentation(nextField)?.controlType === "range" ? seed : null);
  }, [editorIdentity]);

  useEffect(() => {
    if (editorDisclosed) composer.current?.querySelector<HTMLSelectElement>("select")?.focus();
  }, [editorDisclosed, field]);

  function closeEditor() {
    if (editorState) editorDispatch?.({ type: "cancel-editor" });
    else setLocalEditorDisclosed(false);
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

  useEffect(() => {
    if (!editorDisclosed || presentation.controlType !== "range") return;
    let cancelled = false;
    setRangeStats(undefined);
    setRangeStatsReady(false);
    void api.numericFilterStats(field as (typeof QUICK_RANGE_FIELDS)[number], sessionId)
      .then((value) => { if (!cancelled) setRangeStats(value); })
      .catch(() => { if (!cancelled) setRangeStats(undefined); })
      .finally(() => { if (!cancelled) setRangeStatsReady(true); });
    return () => { cancelled = true; };
  }, [editorDisclosed, field, presentation.controlType, sessionId]);

  const needsTwoValues = op === "between" && def.kind !== "datetime";
  const needsValue = !["is-null", "not-null"].includes(op);
  const valueUsesPair = needsValue && op === "between";
  const configuredRaw = raw === "" && !Array.isArray(presentation.defaultValue)
    && presentation.defaultValue !== undefined
    ? String(presentation.defaultValue)
    : raw;
  const candidate = useMemo(() => presentation.controlType === "range" ? rangeCandidate
    : presentation.controlType === "boolean" && raw === "__any__" ? null
    : configuredRaw === UNIDENTIFIED
      ? { field, operator: "is-null" as const, value: null }
      : buildCondition(field, op, configuredRaw, raw2), [configuredRaw, field, op, raw2, rangeCandidate, presentation.controlType]);
  const editorConfigured = candidate !== null;
  const expanded = mode === "inspector" || open;
  const categoryFields = FILTER_CHOICES.filter((choice) => !choice.preset && choice.category === category && choice.field !== "palette_color");
  const rangeFields = categoryFields.map((choice) => choice.field).filter((name) => QUICK_RANGE_FIELDS.some((field) => field === name));
  const specificFields = categoryFields.filter((choice) => !BESPOKE_FILTER_FIELDS.has(choice.field)
    && !(category === "Rating & review" && choice.field === "rating"));
  const rating = draft.find((condition) => condition.field === "rating");
  const ratingThreshold = rating?.operator === ">=" && typeof rating.value === "number" ? rating.value : null;
  const unratedOnly = rating?.operator === "=" && rating.value === 0;

  useEffect(() => {
    if (!controlledEditor || field !== controlledEditor.field) return;
    editorDispatch?.({ type: "update-candidate", candidate });
  }, [candidate, controlledEditor?.field, editorDispatch, field]);

  function setRatingFilter(value: "any" | "unrated" | number) {
    const withoutRating = draft.filter((condition) => condition.field !== "rating");
    if (value === "any") onChange(withoutRating);
    else if (value === "unrated") onChange([...withoutRating, { field: "rating", operator: "=", value: 0 }]);
    else onChange([...withoutRating, { field: "rating", operator: ">=", value }]);
  }

  function selectField(f: string) {
    if (editorState) {
      const existingIndex = draft.findIndex((condition) => condition.field === f);
      editorDispatch?.(existingIndex >= 0
        ? { type: "begin-edit", index: existingIndex }
        : { type: "begin-add", field: f });
      return;
    }
    setLocalEditorDisclosed(true);
    setField(f);
    const first = getFilterPresentation(f)!.operatorOptions[0].op;
    setOp(first);
    setRaw("");
    setRaw2("");
    setRangeCandidate(null);
  }

  function add() {
    if (!candidate) return;
    if (editorState) {
      editorDispatch?.({ type: "update-candidate", candidate });
      editorDispatch?.({ type: "commit-editor" });
      setRaw("");
      setRaw2("");
      root.current?.querySelector<HTMLInputElement>(".filter-search")?.focus();
      return;
    }
    onChange([...draft, candidate]);
    closeEditor();
    setRaw("");
    setRaw2("");
  }

  function valueInput() {
    if (!needsValue) return null;
    if (presentation.controlType === "rating") {
      return (
        <div className="filter-rating-choices" aria-label="Rating value">
          <button type="button" className="filter-rating-unrated" aria-pressed={Number(configuredRaw) === 0}
            onClick={() => setRaw("0")}>Unrated</button>
          {[1, 2, 3, 4, 5].map((star) => <button key={star} type="button" aria-label={`${star} ${star === 1 ? "star" : "stars"}`}
            aria-pressed={Number(configuredRaw) === star} className={Number(configuredRaw) >= star ? "is-on" : ""}
            onClick={() => setRaw(String(star))}>★</button>)}
        </div>
      );
    }
    switch (def.kind) {
      case "palette":
        return null;
      case "bool":
        return (
          <div className="filter-boolean-choices" aria-label={`${def.label} value`}>
            {[{ value: "__any__", label: "Any" }, ...(presentation.valueOptions ?? []).map((option) => ({ value: String(option.value), label: option.label }))].map((option) =>
              <button key={option.value} type="button" aria-pressed={(raw || "true") === option.value}
                onClick={() => setRaw(option.value)}>{option.label}</button>)}
          </div>
        );
      case "text":
        if (hasMetadataOptions) {
          return (
            <OptionPicker label={`${def.label} value`} value={raw} disabled={metadataOptionsLoading}
              options={[
                { value: "", label: metadataOptionsLoading ? "Loading values…" : "Choose from this shoot" },
                ...(metadataOptions?.values.map((option) => ({ value: option.value, label: `${option.value} (${option.count.toLocaleString()})` })) ?? []),
                ...((metadataOptions?.unidentified_count ?? 0) > 0 ? [{ value: UNIDENTIFIED, label: `Unidentified (${metadataOptions!.unidentified_count.toLocaleString()})` }] : []),
              ]}
              onChange={(value) => {
                setRaw(value);
                if (value === UNIDENTIFIED) setOp("is-null");
                else if (op === "is-null" || op === "not-null") setOp("=");
              }} />
          );
        }
        if (def.values) {
          return (
            <OptionPicker label={`${def.label} value`} value={raw} onChange={setRaw}
              options={[{ value: "", label: "Choose…" }, ...(presentation.valueOptions ?? []).map((option) => ({ value: String(option.value), label: option.label }))]} />
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
            <FilterPicker draft={draft} disabled={disabled} includePresets={!category} autoFocus={editorState?.kind === "idle" && autoFocusSearch} onOpenChange={editorState ? (isOpen) => {
              if (isOpen) editorDispatch?.({ type: "open-picker" });
            } : undefined} onSelect={(choice) => {
              if (choice.preset) onChange(toggleQuickFilterPreset(draft, choice.preset));
              else if (choice.field === "palette_color") {
                if (onPaletteSelect) onPaletteSelect();
                else (root.current?.closest("dialog") ?? root.current)?.querySelector<HTMLButtonElement>(".color-swatch")?.focus();
              } else selectField(choice.field);
            }} />
            {!category && <ActiveFilterList draft={draft} onChange={onChange} disabled={disabled} />}
          </div>
          {!category && <ColorSpectrumFilter draft={draft} onChange={onChange} disabled={disabled} />}

          {!category && <section className="quick-presets" aria-labelledby={`${headingId}-quick`}>
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
          {category && specificFields.length > 0 && <div className="filter-category-fields">
            <h4 className="filter-section-title">Specific conditions</h4>
            {specificFields.map((choice) => <button key={choice.id} type="button" disabled={disabled}
              className="filter-category-field" onClick={() => selectField(choice.field)}>
              <span>{choice.label}</span><span>{draft.some((condition) => condition.field === choice.field) ? "Added" : "Any"} <span aria-hidden="true">›</span></span>
            </button>)}
          </div>}
          <div className={`more-filters${editorDisclosed ? " is-open" : ""}`}>
            {editorDisclosed && (
              <div key={editorIdentity || field} className="more-filters-panel" ref={composer} onKeyDown={(event) => {
                if (event.key === "Escape") { event.stopPropagation(); closeEditor(); }
              }}>
                <div className="filterbar-compose">
                  <div className="filter-editor-heading">
                    <strong>{def.label}</strong>
                    <button type="button" className="btn btn-ghost btn-sm" aria-label="Close filter editor"
                      onClick={closeEditor}>×</button>
                  </div>
                  {presentation.controlType === "range" && RANGE_SPECS.find((item) => item.field === field) && (
                    <div className="filter-compose-range">
                      <PrecisionRangeFilter
                        editor
                        spec={RANGE_SPECS.find((item) => item.field === field)!}
                        condition={rangeCandidate ?? undefined}
                        stats={rangeStats}
                        statsReady={rangeStatsReady}
                        disabled={disabled}
                        expanded
                        onToggle={() => {}}
                        onConditionChange={setRangeCandidate}
                      />
                    </div>
                  )}
                  {presentation.controlType !== "boolean" && presentation.controlType !== "range" && (
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
                  )}
                  {needsValue && presentation.controlType !== "range" && (
                    <ComposerControl
                      label={presentation.controlType === "boolean" ? "Show" : presentation.controlType === "rating" ? "Rating" : "Value"}
                      select={false}
                      wide={valueUsesPair}
                      className="filter-compose-control-value"
                    >
                      {valueInput()}
                    </ComposerControl>
                  )}
                  <button type="button" className={`btn btn-sm filterbar-compose-add${category ? "" : " btn-primary"}`} onClick={add} disabled={!editorConfigured || disabled}>
                    {controlledEditor?.intent === "edit" ? "Save change" : "Add filter"}
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
