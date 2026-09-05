import { useEffect, useRef, useState } from "react";
import { api } from "@/lib/ipc";
import type {
  FilterCondition,
  NumericFilterStats,
  QuickNumericFilterField,
} from "@/types/api";
import {
  QUICK_RANGE_FIELDS,
  STANDARD_FILTER_STOPS,
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
}

interface RangeSpec {
  field: QuickRangeField;
  label: string;
  values: readonly number[];
  unit?: string;
  recordedNoun: string;
  missingNoun: string;
}

const MEASURED_VALUES = Array.from({ length: 101 }, (_, index) => index);
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

const RANGE_SPECS: readonly RangeSpec[] = [
  { field: "brightness", label: "Brightness", values: MEASURED_VALUES, recordedNoun: "measured", missingNoun: "unmeasured" },
  { field: "sharpness", label: "Sharpness", values: MEASURED_VALUES, recordedNoun: "measured", missingNoun: "unmeasured" },
  { field: "contrast", label: "Contrast", values: MEASURED_VALUES, recordedNoun: "measured", missingNoun: "unmeasured" },
  { field: "highlight_clipping", label: "Highlight clipping", values: MEASURED_VALUES, unit: "%", recordedNoun: "measured", missingNoun: "unmeasured" },
  { field: "shadow_clipping", label: "Shadow clipping", values: MEASURED_VALUES, unit: "%", recordedNoun: "measured", missingNoun: "unmeasured" },
  { field: "eye_closure_confidence", label: "Eye closure confidence", values: MEASURED_VALUES, unit: "%", recordedNoun: "evaluated", missingNoun: "not evaluated" },
  { field: "iso", label: "ISO", values: STANDARD_FILTER_STOPS.iso, recordedNoun: "recorded", missingNoun: "not recorded" },
  { field: "focal_length", label: "Focal length", values: STANDARD_FILTER_STOPS.focal_length, unit: " mm", recordedNoun: "recorded", missingNoun: "not recorded" },
];

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
  if (typeof condition.value === "number") {
    return `${condition.operator} ${formatValue(condition.value, spec.unit)}`;
  }
  return "Custom condition";
}

interface RangeFilterRowProps {
  spec: RangeSpec;
  condition?: FilterCondition;
  stats?: NumericFilterStats;
  statsReady: boolean;
  disabled?: boolean;
  draft: FilterCondition[];
  onChange: (conditions: FilterCondition[]) => void;
  expanded: boolean;
  onToggle: () => void;
}

function RangeNumberInput({ label, value, min, max, disabled, onCommit }: {
  label: string; value: number; min: number; max: number; disabled?: boolean;
  onCommit: (value: number) => void;
}) {
  const [raw, setRaw] = useState(String(value));
  useEffect(() => setRaw(String(value)), [value]);
  function commit() {
    const parsed = Number(raw);
    if (!raw.trim() || !Number.isFinite(parsed)) { setRaw(String(value)); return; }
    const bounded = Math.max(min, Math.min(max, parsed));
    onCommit(bounded);
    setRaw(String(value));
  }
  return <input className="input" type="number" aria-label={label} min={min} max={max} value={raw}
    disabled={disabled} onChange={(event) => setRaw(event.target.value)} onBlur={commit}
    onKeyDown={(event) => {
      if (event.key === "Enter") event.currentTarget.blur();
      if (event.key === "Escape") { event.stopPropagation(); setRaw(String(value)); }
    }} />;
}

function RangeFilterRow({
  spec,
  condition,
  stats,
  statsReady,
  disabled,
  draft,
  onChange,
  expanded,
  onToggle,
}: RangeFilterRowProps) {
  const domainLower = spec.values[0];
  const domainUpper = spec.values[spec.values.length - 1];
  const bounds = quickRangeBounds(condition, domainLower, domainUpper);
  // A compact inclusive scrubber cannot faithfully edit strict operators or
  // values outside its stops. Keep those exact conditions in the composer.
  const parsed = { ...bounds, editable: bounds.editable
    && !["<", ">"].includes(condition?.operator ?? "")
    && spec.values.includes(bounds.lower) && spec.values.includes(bounds.upper) };
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

  const noRecordedValues = !stats || stats.recorded_count === 0;
  const sliderDisabled = disabled || noRecordedValues || parsed.missingOnly || !parsed.editable;
  const lowerPercent = lowerIndex / (spec.values.length - 1) * 100;
  const upperPercent = upperIndex / (spec.values.length - 1) * 100;
  const isFilteredRange = Boolean(condition && !parsed.missingOnly && parsed.editable);

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
    onChange(replaceFieldConditions(draft, spec.field, replacement));
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
    ? `${stats.recorded_count.toLocaleString()} ${spec.recordedNoun} · ${stats.missing_count.toLocaleString()} ${spec.missingNoun}`
    : statsReady ? "Values unavailable" : "Checking local values…";
  const detailId = `range-filter-${spec.field}`;
  return (
    <div className={`range-filter-row${condition ? " has-filter" : ""}`}>
      <button
        type="button"
        className="range-filter-heading"
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
      </button>

      <div className={`range-filter-reveal${expanded ? " is-open" : ""}`} aria-hidden={!expanded}>
        <fieldset className="range-filter-detail" id={detailId} aria-label={`${spec.label} range`} disabled={!expanded || disabled}>
          <p className="range-filter-note">{availability}</p>
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
                style={{ left: `clamp(24px, ${activeHandle === "lower" ? lowerPercent : upperPercent}%, calc(100% - 24px))` }}
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
            <label>Min <RangeNumberInput label={`${spec.label} minimum value`}
              min={domainLower} max={spec.values[upperIndex]} value={spec.values[lowerIndex]}
              disabled={sliderDisabled} onCommit={(value) => {
                updateLower(nearestValueIndex(spec.values, value)); commitRange();
              }} /></label>
            <label>Max <RangeNumberInput label={`${spec.label} maximum value`}
              min={spec.values[lowerIndex]} max={domainUpper} value={spec.values[upperIndex]}
              disabled={sliderDisabled} onCommit={(value) => {
                updateUpper(nearestValueIndex(spec.values, value)); commitRange();
              }} /></label>
          </div>

          {!parsed.editable && !parsed.missingOnly && (
            <p className="range-filter-note">This custom condition remains unchanged. Use Search filters to add an exact condition.</p>
          )}
          <div className="range-filter-actions">
            <button
              type="button"
              className={parsed.missingOnly ? "is-active" : ""}
              disabled={disabled || !stats || stats.missing_count === 0}
              onClick={() => onChange(replaceFieldConditions(draft, spec.field, {
                field: spec.field,
                operator: "is-null",
                value: null,
              }))}
            >
              {spec.missingNoun[0].toUpperCase()}{spec.missingNoun.slice(1)} only
            </button>
            <button
              type="button"
              disabled={disabled || !condition}
              onClick={() => onChange(replaceFieldConditions(draft, spec.field, null))}
            >
              Reset to any
            </button>
          </div>
        </fieldset>
      </div>
    </div>
  );
}

export function QuickFilterControls({ draft, onChange, disabled, sessionId }: QuickFilterControlsProps) {
  const [stats, setStats] = useState<Partial<Record<QuickNumericFilterField, NumericFilterStats>>>({});
  const [statsReady, setStatsReady] = useState(false);
  const [expandedField, setExpandedField] = useState<string | null>(null);

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
    <section className="quick-filters" aria-labelledby="measured-filter-heading">
      <h3 className="filter-section-title" id="measured-filter-heading">Image properties</h3>
        <div className="quick-filter-content" id="measured-filter-controls">
          {measurementsUnavailable && (
            <div className="quick-filter-note">
              Technical measurements become available after Analyze photos finishes. Eye confidence becomes available after the optional local face and eye pass.
            </div>
          )}
          <div className="range-filter-list">
            {RANGE_SPECS.map((spec) => {
              const condition = draft.find((item) => item.field === spec.field);
              return (
                <RangeFilterRow
                  key={spec.field}
                  expanded={expandedField === spec.field}
                  onToggle={() => setExpandedField((current) => current === spec.field ? null : spec.field)}
                  spec={spec}
                  condition={condition}
                  stats={stats[spec.field]}
                  statsReady={statsReady}
                  disabled={disabled}
                  draft={draft}
                  onChange={onChange}
                />
              );
            })}
          </div>
        </div>
    </section>
  );
}
