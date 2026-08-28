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

const RANGE_SPECS: readonly RangeSpec[] = [
  { field: "brightness", label: "Brightness", values: MEASURED_VALUES, recordedNoun: "measured", missingNoun: "unmeasured" },
  { field: "sharpness", label: "Sharpness", values: MEASURED_VALUES, recordedNoun: "measured", missingNoun: "unmeasured" },
  { field: "contrast", label: "Contrast", values: MEASURED_VALUES, recordedNoun: "measured", missingNoun: "unmeasured" },
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

function conditionSummary(condition: FilterCondition | undefined, unit = "", missingNoun = "unmeasured"): string {
  if (!condition) return "Any";
  if (condition.operator === "is-null") return `${missingNoun[0].toUpperCase()}${missingNoun.slice(1)} only`;
  if (condition.operator === "not-null") return "Recorded only";
  if (condition.operator === "between" && Array.isArray(condition.value) && condition.value.length === 2) {
    return `${formatValue(Number(condition.value[0]), unit)}–${formatValue(Number(condition.value[1]), unit)}`;
  }
  if (typeof condition.value === "number") {
    const symbol: Partial<Record<FilterCondition["operator"], string>> = {
      "=": "=", "!=": "≠", ">": ">", ">=": "≥", "<": "<", "<=": "≤",
    };
    return `${symbol[condition.operator] ?? condition.operator} ${formatValue(condition.value, unit)}`;
  }
  return "Custom condition";
}

function tickPositions(valueCount: number): number[] {
  const intervals = Math.min(valueCount - 1, 10);
  return Array.from({ length: intervals + 1 }, (_, index) => index / intervals * 100);
}

interface RangeFilterRowProps {
  spec: RangeSpec;
  condition?: FilterCondition;
  stats?: NumericFilterStats;
  statsReady: boolean;
  disabled?: boolean;
  draft: FilterCondition[];
  onChange: (conditions: FilterCondition[]) => void;
}

function RangeFilterRow({
  spec,
  condition,
  stats,
  statsReady,
  disabled,
  draft,
  onChange,
}: RangeFilterRowProps) {
  const domainLower = spec.values[0];
  const domainUpper = spec.values[spec.values.length - 1];
  const parsed = quickRangeBounds(condition, domainLower, domainUpper);
  const initialLower = nearestValueIndex(spec.values, parsed.lower);
  const initialUpper = Math.max(initialLower, nearestValueIndex(spec.values, parsed.upper));
  const [lowerIndex, setLowerIndex] = useState(initialLower);
  const [upperIndex, setUpperIndex] = useState(initialUpper);
  const boundsRef = useRef({ lower: initialLower, upper: initialUpper });
  const lastCommitRef = useRef(JSON.stringify(condition ?? null));
  const noRecordedValues = !stats || stats.recorded_count === 0;
  const sliderDisabled = disabled || noRecordedValues || parsed.missingOnly || !parsed.editable;
  const lowerPercent = lowerIndex / (spec.values.length - 1) * 100;
  const upperPercent = upperIndex / (spec.values.length - 1) * 100;
  const isFilteredRange = Boolean(condition && !parsed.missingOnly && parsed.editable);

  function updateLower(next: number) {
    const lower = Math.min(next, boundsRef.current.upper);
    boundsRef.current = { ...boundsRef.current, lower };
    setLowerIndex(lower);
  }

  function updateUpper(next: number) {
    const upper = Math.max(next, boundsRef.current.lower);
    boundsRef.current = { ...boundsRef.current, upper };
    setUpperIndex(upper);
  }

  function commitRange() {
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

  function commitKeyboard(event: React.KeyboardEvent<HTMLInputElement>) {
    if (["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End", "PageUp", "PageDown"].includes(event.key)) {
      commitRange();
    }
  }

  const availability = stats
    ? `${stats.recorded_count.toLocaleString()} ${spec.recordedNoun} · ${stats.missing_count.toLocaleString()} ${spec.missingNoun}`
    : statsReady ? "Values unavailable" : "Checking local values…";
  const minimum = stats?.minimum == null ? null : formatValue(Math.round(stats.minimum * 10) / 10, spec.unit);
  const maximum = stats?.maximum == null ? null : formatValue(Math.round(stats.maximum * 10) / 10, spec.unit);

  return (
    <div className={`range-filter-row${condition ? " has-filter" : ""}`}>
      <div className="range-filter-heading">
        <span>{spec.label}</span>
        <span className={`range-filter-summary mono${condition ? " is-active" : ""}`}>
          {conditionSummary(condition, spec.unit, spec.missingNoun)}
        </span>
      </div>

      <div className="range-filter-detail" id={`range-filter-${spec.field}`}>
          <div className="range-filter-availability">
            <span>{availability}</span>
            {minimum !== null && maximum !== null && <span className="mono">{minimum}–{maximum}</span>}
          </div>

          <div className={`range-scrubber${isFilteredRange ? " is-filtered" : ""}${sliderDisabled ? " is-disabled" : ""}${lowerIndex === upperIndex ? " is-collapsed" : ""}`}>
            <output className="range-bound mono">{formatValue(spec.values[lowerIndex], spec.unit)}</output>
            <output className="range-bound mono">{formatValue(spec.values[upperIndex], spec.unit)}</output>
            <div className="range-track" aria-hidden="true">
              <span className="range-track-base" />
              <span className="range-track-selected" style={{ left: `${lowerPercent}%`, right: `${100 - upperPercent}%` }} />
              {tickPositions(spec.values.length).map((position) => (
                <span className="range-track-tick" key={position} style={{ left: `${position}%` }} />
              ))}
            </div>
            <input
              className="range-input range-input-lower"
              type="range"
              min={0}
              max={spec.values.length - 1}
              value={lowerIndex}
              disabled={sliderDisabled}
              aria-label={`${spec.label} minimum`}
              aria-valuetext={`Minimum ${formatValue(spec.values[lowerIndex], spec.unit)}`}
              onChange={(event) => updateLower(Number(event.target.value))}
              onPointerUp={commitRange}
              onPointerCancel={commitRange}
              onBlur={commitRange}
              onKeyUp={commitKeyboard}
            />
            <input
              className="range-input range-input-upper"
              type="range"
              min={0}
              max={spec.values.length - 1}
              value={upperIndex}
              disabled={sliderDisabled}
              aria-label={`${spec.label} maximum`}
              aria-valuetext={`Maximum ${formatValue(spec.values[upperIndex], spec.unit)}`}
              onChange={(event) => updateUpper(Number(event.target.value))}
              onPointerUp={commitRange}
              onPointerCancel={commitRange}
              onBlur={commitRange}
              onKeyUp={commitKeyboard}
            />
          </div>

          {!parsed.editable && !parsed.missingOnly && (
            <p className="range-filter-note">This custom condition remains unchanged. Edit it in More filters.</p>
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
      </div>
    </div>
  );
}

export function QuickFilterControls({ draft, onChange, disabled, sessionId }: QuickFilterControlsProps) {
  const [stats, setStats] = useState<Partial<Record<QuickNumericFilterField, NumericFilterStats>>>({});
  const [statsReady, setStatsReady] = useState(false);

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
  }, [sessionId]);

  const measurementsUnavailable = statsReady
    && (["brightness", "sharpness", "contrast"] as const)
      .every((field) => stats[field]?.recorded_count === 0);

  return (
    <section className="quick-filters" aria-labelledby="measured-filter-heading">
      <div className="quick-filter-intro">
        <strong id="measured-filter-heading">Measured filters</strong>
        <span>Drag either edge to refine the visible photographs</span>
      </div>
      {measurementsUnavailable && (
        <div className="quick-filter-note">
          Brightness, sharpness and contrast become available after Analyze photos finishes.
        </div>
      )}
      <div className="range-filter-list">
        {RANGE_SPECS.map((spec) => {
          const condition = draft.find((item) => item.field === spec.field);
          return (
            <RangeFilterRow
              key={`${spec.field}-${JSON.stringify(condition ?? null)}`}
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
    </section>
  );
}
