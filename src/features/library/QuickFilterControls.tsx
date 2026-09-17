import { Fragment, useEffect, useId, useRef, useState, type ReactNode } from "react";
import { api } from "@/lib/ipc";
import type {
  FilterCondition,
  NumericFilterStats,
  QuickNumericFilterField,
} from "@/types/api";
import {
  QUICK_RANGE_FIELDS,
  getFilterPresentation,
  quickRangeBounds,
  quickRangeCondition,
  replaceFieldConditions,
  type QuickRangeField,
} from "./filterFields";

interface QuickFilterControlsProps {
  draft: FilterCondition[];
  onChange: (conditions: FilterCondition[]) => void;
  disabled?: boolean;
  sessionId: number | null;
  fields?: readonly string[];
  renderAfterField?: (field: QuickRangeField) => ReactNode;
}

export interface RangeSpec {
  field: QuickRangeField;
  label: string;
  values: readonly number[];
  unit?: string;
  recordedNoun: string;
  missingNoun: string;
}

const RANGE_INTERACTION_KEYS = new Set([
  "ArrowLeft",
  "ArrowRight",
  "ArrowUp",
  "ArrowDown",
  "Home",
  "End",
  "PageUp",
  "PageDown",
]);

const RANGE_NOUNS: Record<QuickRangeField, Pick<RangeSpec, "recordedNoun" | "missingNoun">> = {
  brightness: { recordedNoun: "measured", missingNoun: "unmeasured" },
  sharpness: { recordedNoun: "measured", missingNoun: "unmeasured" },
  contrast: { recordedNoun: "measured", missingNoun: "unmeasured" },
  saturation: { recordedNoun: "measured", missingNoun: "unmeasured" },
  highlight_clipping: { recordedNoun: "measured", missingNoun: "unmeasured" },
  shadow_clipping: { recordedNoun: "measured", missingNoun: "unmeasured" },
  eye_closure_confidence: { recordedNoun: "evaluated", missingNoun: "not evaluated" },
  iso: { recordedNoun: "recorded", missingNoun: "not recorded" },
  focal_length: { recordedNoun: "recorded", missingNoun: "not recorded" },
};

export const RANGE_SPECS: readonly RangeSpec[] = QUICK_RANGE_FIELDS.map((field) => {
  const presentation = getFilterPresentation(field)!;
  return {
    field,
    label: presentation.label.replace(" (mm)", ""),
    values: presentation.values!,
    unit: presentation.unit,
    ...RANGE_NOUNS[field],
  };
});

function nearestValueIndex(values: readonly number[], value: number): number {
  return values.reduce(
    (closest, candidate, index) => Math.abs(candidate - value) < Math.abs(values[closest] - value) ? index : closest,
    0,
  );
}

function formatValue(value: number, unit = ""): string {
  return `${value.toLocaleString()}${unit}`;
}

function conditionSummary(condition: FilterCondition | undefined, spec: RangeSpec): string {
  if (!condition) return "Any";
  if (condition.operator === "is-null") {
    return `${spec.missingNoun[0].toUpperCase()}${spec.missingNoun.slice(1)} only`;
  }
  if (condition.operator === "not-null") {
    return `${spec.recordedNoun[0].toUpperCase()}${spec.recordedNoun.slice(1)} only`;
  }
  if (
    condition.operator === "between"
    && Array.isArray(condition.value)
    && condition.value.length === 2
    && condition.value.every((value) => typeof value === "number")
  ) {
    return `${formatValue(condition.value[0] as number, spec.unit)}–${formatValue(condition.value[1] as number, spec.unit)}`;
  }
  if (typeof condition.value === "number") return `${({
    "=": "Exactly", ">=": "At least", ">": "More than", "<=": "At most", "<": "Less than",
  } as Partial<Record<FilterCondition["operator"], string>>)[condition.operator] ?? condition.operator} ${formatValue(condition.value, spec.unit)}`;
  return "Custom condition";
}

interface PrecisionRangeFilterProps {
  spec: RangeSpec;
  condition?: FilterCondition;
  stats?: NumericFilterStats;
  statsReady: boolean;
  disabled?: boolean;
  onConditionChange: (condition: FilterCondition | null) => void;
  expanded: boolean;
  onToggle: () => void;
  editor?: boolean;
}

function RangeNumberInput({ label, value, min, max, step, disabled, onCommit }: {
  label: string; value: number; min: number; max: number; step: number | "any"; disabled?: boolean;
  onCommit: (value: number) => void;
}) {
  const [raw, setRaw] = useState(String(value));
  useEffect(() => setRaw(String(value)), [value]);
  function commit() {
    const parsed = Number(raw);
    if (!raw.trim() || !Number.isFinite(parsed)) { setRaw(String(value)); return; }
    const bounded = Math.max(min, Math.min(max, parsed));
    onCommit(bounded);
    setRaw(String(bounded));
  }
  return <input className="input" type="number" aria-label={label} min={min} max={max} step={step} value={raw}
    disabled={disabled} onChange={(event) => setRaw(event.target.value)} onBlur={commit}
    onKeyDown={(event) => {
      if (event.key === "Enter") event.currentTarget.blur();
      if (event.key === "Escape") { event.stopPropagation(); setRaw(String(value)); }
    }} />;
}

export function PrecisionRangeFilter({
  spec,
  condition,
  stats,
  statsReady,
  disabled,
  onConditionChange,
  expanded,
  onToggle,
  editor = false,
}: PrecisionRangeFilterProps) {
  const domainLower = spec.values[0];
  const domainUpper = spec.values[spec.values.length - 1];
  const bounds = quickRangeBounds(condition, domainLower, domainUpper);
  // A compact inclusive scrubber cannot faithfully edit strict operators or
  // values outside its stops. Keep those exact conditions in the composer.
  const parsed = { ...bounds, editable: bounds.editable
    && !["<", ">"].includes(condition?.operator ?? "") };
  const initialLower = nearestValueIndex(spec.values, parsed.lower);
  const initialUpper = Math.max(initialLower, nearestValueIndex(spec.values, parsed.upper));
  const [lowerIndex, setLowerIndex] = useState(initialLower);
  const [upperIndex, setUpperIndex] = useState(initialUpper);
  const [activeHandle, setActiveHandle] = useState<"lower" | "upper" | null>(null);
  const boundsRef = useRef({ lower: initialLower, upper: initialUpper });
  const rangeChanged = useRef(false);
  const lastCommitRef = useRef(JSON.stringify(condition ?? null));
  useEffect(() => {
    setLowerIndex(initialLower);
    setUpperIndex(initialUpper);
    boundsRef.current = { lower: initialLower, upper: initialUpper };
    rangeChanged.current = false;
    lastCommitRef.current = JSON.stringify(condition ?? null);
  }, [initialLower, initialUpper, condition]);

  const noRecordedValues = stats?.recorded_count === 0;
  const sliderDisabled = disabled || !stats || noRecordedValues || parsed.missingOnly || !parsed.editable;
  const lowerPercent = lowerIndex / (spec.values.length - 1) * 100;
  const upperPercent = upperIndex / (spec.values.length - 1) * 100;
  const isFilteredRange = Boolean(condition && !parsed.missingOnly && parsed.editable);
  const isExact = condition?.operator === "=" && typeof condition.value === "number";

  function updateLower(next: number) {
    const lower = Math.min(next, boundsRef.current.upper);
    rangeChanged.current ||= lower !== boundsRef.current.lower;
    boundsRef.current = { ...boundsRef.current, lower };
    setLowerIndex(lower);
  }

  function updateUpper(next: number) {
    const upper = Math.max(next, boundsRef.current.lower);
    rangeChanged.current ||= upper !== boundsRef.current.upper;
    boundsRef.current = { ...boundsRef.current, upper };
    setUpperIndex(upper);
  }

  function commitRange() {
    // Focusing and leaving a control must not normalize an exact condition
    // (or remove a not-null filter whose displayed handles span the domain).
    if (!rangeChanged.current || sliderDisabled) return;
    rangeChanged.current = false;
    const replacement = quickRangeCondition(
      spec.field,
      spec.values[boundsRef.current.lower],
      spec.values[boundsRef.current.upper],
      domainLower,
      domainUpper,
    );
    const signature = JSON.stringify(replacement);
    if (signature === lastCommitRef.current) return;
    lastCommitRef.current = signature;
    onConditionChange(replacement);
  }

  function commitExact(handle: "lower" | "upper", value: number) {
    const lower = handle === "lower" ? value : bounds.lower;
    const upper = handle === "upper" ? value : bounds.upper;
    if (lower > upper) return;
    // Exact input is independent of the slider's useful photographic stops.
    // A typed ISO of 640, for example, must not become the 800 stop.
    const replacement = lower === upper
      ? { field: spec.field, operator: "=" as const, value: lower }
      : quickRangeCondition(spec.field, lower, upper, domainLower, domainUpper);
    onConditionChange(replacement);
  }

  function beginKeyboardInteraction(
    event: React.KeyboardEvent<HTMLInputElement>,
    handle: "lower" | "upper",
  ) {
    if (RANGE_INTERACTION_KEYS.has(event.key)) setActiveHandle(handle);
  }

  function finishKeyboardInteraction(event: React.KeyboardEvent<HTMLInputElement>) {
    if (!RANGE_INTERACTION_KEYS.has(event.key)) return;
    setActiveHandle(null);
    commitRange();
  }

  function finishInteraction() {
    setActiveHandle(null);
    commitRange();
  }

  const availability = stats
    ? noRecordedValues
      ? "Not recorded in this shoot"
      : `${stats.recorded_count.toLocaleString()} ${spec.recordedNoun}${stats.missing_count ? ` · ${stats.missing_count.toLocaleString()} ${spec.missingNoun}` : ""}`
    : statsReady ? "Values unavailable" : "Checking local values…";
  const exactInputStep = spec.values.every((value, index) => index === 0 || value - spec.values[index - 1] === 1)
    ? 1
    : "any";
  const detailId = useId();
  return (
    <div className={`range-filter-row${condition ? " has-filter" : ""}${editor ? " is-editor" : ""}`}>
      {!editor && <button
        type="button"
        className="range-filter-heading"
        data-filter-field={spec.field}
        aria-expanded={expanded}
        aria-controls={detailId}
        onClick={onToggle}
      >
        <span className="range-filter-copy">
          <strong>{spec.label}</strong>
        </span>
        <span className="range-filter-state">
          <span className={`range-filter-summary mono${condition ? " is-active" : ""}`}>
            {conditionSummary(condition, spec)}
          </span>
          <span className="range-filter-chevron" aria-hidden="true">⌄</span>
        </span>
      </button>}

      <div className={`range-filter-reveal${expanded ? " is-open" : ""}`} aria-hidden={!expanded}>
        <fieldset className="range-filter-detail" id={detailId} aria-label={`${spec.label} range`} disabled={!expanded || disabled}>
          <p className="range-filter-note">{availability}</p>
          {!noRecordedValues && (
            <>
              <div
                className={`range-scrubber${isFilteredRange ? " is-filtered" : ""}${sliderDisabled ? " is-disabled" : ""}${lowerIndex === upperIndex ? " is-collapsed" : ""}`}
                data-field={spec.field}
              >
                <div className="range-track" aria-hidden="true">
                  <span className="range-track-base" />
                  <span className="range-track-selected" style={{ left: `${lowerPercent}%`, right: `${100 - upperPercent}%` }} />
                </div>
                {activeHandle && (
                  <output
                    className="range-value-bubble mono"
                    style={{ left: `calc(var(--range-inset) + (100% - 2 * var(--range-inset)) * ${(activeHandle === "lower" ? lowerPercent : upperPercent) / 100})` }}
                  >
                    {formatValue(spec.values[activeHandle === "lower" ? lowerIndex : upperIndex], spec.unit)}
                  </output>
                )}
                <input
                  className={`range-input range-input-lower${activeHandle === "lower" ? " is-active" : ""}`}
                  type="range"
                  min={0}
                  max={spec.values.length - 1}
                  value={lowerIndex}
                  disabled={sliderDisabled}
                  aria-label={`${spec.label} minimum`}
                  aria-valuetext={`Minimum ${formatValue(spec.values[lowerIndex], spec.unit)}`}
                  onChange={(event) => updateLower(Number(event.target.value))}
                  onPointerDown={() => setActiveHandle("lower")}
                  onPointerUp={finishInteraction}
                  onPointerCancel={finishInteraction}
                  onBlur={finishInteraction}
                  onKeyDown={(event) => beginKeyboardInteraction(event, "lower")}
                  onKeyUp={finishKeyboardInteraction}
                />
                <input
                  className={`range-input range-input-upper${activeHandle === "upper" ? " is-active" : ""}`}
                  type="range"
                  min={0}
                  max={spec.values.length - 1}
                  value={upperIndex}
                  disabled={sliderDisabled}
                  aria-label={`${spec.label} maximum`}
                  aria-valuetext={`Maximum ${formatValue(spec.values[upperIndex], spec.unit)}`}
                  onChange={(event) => updateUpper(Number(event.target.value))}
                  onPointerDown={() => setActiveHandle("upper")}
                  onPointerUp={finishInteraction}
                  onPointerCancel={finishInteraction}
                  onBlur={finishInteraction}
                  onKeyDown={(event) => beginKeyboardInteraction(event, "upper")}
                  onKeyUp={finishKeyboardInteraction}
                />
              </div>

              <div className="range-numeric-values">
                {isExact ? <label>Exact <RangeNumberInput label={`${spec.label} exact value`}
                  min={domainLower} max={domainUpper} step={exactInputStep} value={bounds.lower}
                  disabled={sliderDisabled} onCommit={(value) => onConditionChange({ field: spec.field, operator: "=", value })} /></label> : <>
                <label>Min <RangeNumberInput label={`${spec.label} minimum value`}
                  min={domainLower} max={bounds.upper} step={exactInputStep} value={bounds.lower}
                  disabled={sliderDisabled} onCommit={(value) => {
                    commitExact("lower", value);
                  }} /></label>
                <label>Max <RangeNumberInput label={`${spec.label} maximum value`}
                  min={bounds.lower} max={domainUpper} step={exactInputStep} value={bounds.upper}
                  disabled={sliderDisabled} onCommit={(value) => {
                    commitExact("upper", value);
                  }} /></label>
                </>}
              </div>
            </>
          )}

          {!parsed.editable && !parsed.missingOnly && (
            <p className="range-filter-note">This custom condition remains unchanged. Edit its staged filter for precise options.</p>
          )}
          <div className="range-filter-actions">
            {!noRecordedValues && parsed.editable && !parsed.missingOnly && (
              <button type="button" className={`range-filter-exact${isExact ? " is-active" : ""}`}
                aria-pressed={isExact} disabled={sliderDisabled}
                onClick={() => onConditionChange({ field: spec.field, operator: "=", value: spec.values[lowerIndex === 0 ? Math.floor(spec.values.length / 2) : lowerIndex] })}>
                Exact value
              </button>
            )}
            <button
              type="button"
              className={`range-filter-missing${parsed.missingOnly ? " is-active" : ""}`}
              disabled={disabled || !stats || stats.missing_count === 0}
              onClick={() => onConditionChange({
                field: spec.field,
                operator: "is-null",
                value: null,
              })}
            >
              {spec.missingNoun[0].toUpperCase()}{spec.missingNoun.slice(1)} only
            </button>
            <button
              type="button"
              className="range-filter-reset"
              disabled={disabled || !condition}
              onClick={() => onConditionChange(null)}
            >
              Reset to any
            </button>
          </div>
        </fieldset>
      </div>
    </div>
  );
}

export function QuickFilterControls({ draft, onChange, disabled, sessionId, fields, renderAfterField }: QuickFilterControlsProps) {
  const headingId = useId();
  const [stats, setStats] = useState<Partial<Record<QuickNumericFilterField, NumericFilterStats>>>({});
  const [statsReady, setStatsReady] = useState(false);
  const [expandedFields, setExpandedFields] = useState<Set<QuickRangeField>>(() => new Set());

  function toggleExpandedField(field: QuickRangeField) {
    setExpandedFields((current) => {
      const next = new Set(current);
      if (next.has(field)) next.delete(field);
      else next.add(field);
      return next;
    });
  }

  useEffect(() => {
    let cancelled = false;
    setStats({});
    setStatsReady(false);
    void Promise.all(QUICK_RANGE_FIELDS.map(async (field) => {
      try {
        const value = await api.numericFilterStats(field, sessionId);
        if (!cancelled) setStats((current) => ({ ...current, [field]: value }));
      } catch {
        // The controls stay honest and unavailable when local statistics fail.
      }
    })).then(() => { if (!cancelled) setStatsReady(true); });
    return () => { cancelled = true; };
  }, [sessionId, disabled]);

  const measurementsUnavailable = statsReady
    && (["brightness", "sharpness", "contrast", "highlight_clipping", "shadow_clipping"] as const)
      .every((field) => stats[field]?.recorded_count === 0);

  return (
    <section className="quick-filters" aria-labelledby={headingId}>
      <h3 className="filter-section-title" id={headingId}>Measured characteristics</h3>
        <div className="quick-filter-content">
          {measurementsUnavailable && (
            <div className="quick-filter-note">
              Technical measurements become available after Analyze photos finishes. Eye confidence becomes available after the optional local face and eye pass.
            </div>
          )}
          <div className="range-filter-list">
            {RANGE_SPECS.filter((spec) => !fields || fields.includes(spec.field)).map((spec) => {
              const condition = draft.find((item) => item.field === spec.field);
              return (
                <Fragment key={spec.field}>
                  <PrecisionRangeFilter
                    expanded={expandedFields.has(spec.field)}
                    onToggle={() => toggleExpandedField(spec.field)}
                    spec={spec}
                    condition={condition}
                    stats={stats[spec.field]}
                    statsReady={statsReady}
                    disabled={disabled}
                    onConditionChange={(replacement) => onChange(replaceFieldConditions(draft, spec.field, replacement))}
                  />
                  {renderAfterField?.(spec.field)}
                </Fragment>
              );
            })}
          </div>
        </div>
    </section>
  );
}
