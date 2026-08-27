import { useEffect, useState } from "react";
import { api } from "@/lib/ipc";
import type {
  FilterCondition,
  NumericFilterStats,
  QuickNumericFilterField,
} from "@/types/api";
import {
  STANDARD_RANGE_STOPS,
  VISUAL_BANDS,
  activeVisualBand,
  replaceFieldConditions,
  visualBandCondition,
  type VisualQuickField,
} from "./filterFields";

interface QuickFilterControlsProps {
  draft: FilterCondition[];
  onChange: (conditions: FilterCondition[]) => void;
  disabled?: boolean;
  sessionId: number | null;
}

const QUICK_FIELDS: QuickNumericFilterField[] = [
  "brightness",
  "sharpness",
  "contrast",
  "iso",
  "focal_length",
];

function Availability({
  stats,
  noun = "measured",
  missingNoun = "unmeasured",
  ready = false,
}: {
  stats?: NumericFilterStats;
  noun?: string;
  missingNoun?: string;
  ready?: boolean;
}) {
  if (!stats) return <span className="faint">{ready ? "Values unavailable" : "Checking local values…"}</span>;
  return (
    <span className="faint">
      {stats.recorded_count.toLocaleString()} {noun} · {stats.missing_count.toLocaleString()} {missingNoun}
    </span>
  );
}

interface RangeControlProps {
  field: "iso" | "focal_length";
  label: string;
  unit?: string;
  stops: readonly number[];
  stats?: NumericFilterStats;
  statsReady: boolean;
  draft: FilterCondition[];
  onChange: (conditions: FilterCondition[]) => void;
  disabled?: boolean;
}

function StandardRangeControl({ field, label, unit = "", stops, stats, statsReady, draft, onChange, disabled }: RangeControlProps) {
  const existing = draft.find((condition) => condition.field === field);
  const existingRange = existing?.operator === "between" && Array.isArray(existing.value)
    ? existing.value as [number, number]
    : null;
  const initialLow = existingRange ? Math.max(0, stops.indexOf(existingRange[0])) : 0;
  const initialHigh = existingRange ? stops.indexOf(existingRange[1]) : stops.length - 1;
  const [lowIndex, setLowIndex] = useState(initialLow >= 0 ? initialLow : 0);
  const [highIndex, setHighIndex] = useState(initialHigh >= 0 ? initialHigh : stops.length - 1);

  useEffect(() => {
    if (!existingRange) return;
    const low = stops.indexOf(existingRange[0]);
    const high = stops.indexOf(existingRange[1]);
    if (low >= 0) setLowIndex(low);
    if (high >= 0) setHighIndex(high);
  }, [existingRange?.[0], existingRange?.[1], stops]);

  const unmeasured = existing?.operator === "is-null";
  const rangeActive = existingRange !== null;
  const noRecordedValues = !stats || stats.recorded_count === 0;

  function applyRange() {
    onChange(replaceFieldConditions(draft, field, {
      field,
      operator: "between",
      value: [stops[lowIndex], stops[highIndex]],
    }));
  }

  return (
    <div className="quick-range-card">
      <div className="quick-filter-card-head">
        <strong>{label}</strong>
        <Availability stats={stats} noun="recorded" missingNoun="not recorded" ready={statsReady} />
      </div>
      <div className="quick-range-values mono">
        <span>{stops[lowIndex].toLocaleString()}{unit}</span>
        <span>to</span>
        <span>{stops[highIndex].toLocaleString()}{unit}</span>
      </div>
      <div className="quick-range-sliders">
        <input
          type="range"
          min={0}
          max={stops.length - 1}
          value={lowIndex}
          disabled={disabled || noRecordedValues}
          aria-label={`${label} minimum`}
          onChange={(event) => setLowIndex(Math.min(Number(event.target.value), highIndex))}
        />
        <input
          type="range"
          min={0}
          max={stops.length - 1}
          value={highIndex}
          disabled={disabled || noRecordedValues}
          aria-label={`${label} maximum`}
          onChange={(event) => setHighIndex(Math.max(Number(event.target.value), lowIndex))}
        />
      </div>
      {stats?.minimum !== null && stats?.minimum !== undefined && stats.maximum !== null && (
        <div className="faint mono quick-observed-range">
          Observed {stats.minimum.toLocaleString()}–{stats.maximum.toLocaleString()}{unit}
        </div>
      )}
      <div className="quick-filter-actions">
        <button
          className={`btn btn-sm${rangeActive ? " btn-primary" : ""}`}
          onClick={applyRange}
          disabled={disabled || noRecordedValues}
        >
          Apply range
        </button>
        <button
          className={`btn btn-sm${unmeasured ? " btn-primary" : ""}`}
          onClick={() => onChange(replaceFieldConditions(draft, field, { field, operator: "is-null", value: null }))}
          disabled={disabled || !stats || stats.missing_count === 0}
        >
          Not recorded
        </button>
        <button
          className="btn btn-ghost btn-sm"
          onClick={() => onChange(replaceFieldConditions(draft, field, null))}
          disabled={disabled || !existing}
        >
          Any
        </button>
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
    void Promise.all(QUICK_FIELDS.map(async (field) => {
      try {
        const value = await api.numericFilterStats(field, sessionId);
        if (!cancelled) setStats((current) => ({ ...current, [field]: value }));
      } catch {
        // The controls stay honest and unavailable when local statistics fail.
      }
    })).then(() => { if (!cancelled) setStatsReady(true); });
    return () => { cancelled = true; };
  }, [sessionId]);

  function setVisual(field: VisualQuickField, band: "low" | "mid" | "high" | "unmeasured" | null) {
    const replacement = band === null
      ? null
      : band === "unmeasured"
        ? { field, operator: "is-null" as const, value: null }
        : visualBandCondition(field, band);
    onChange(replaceFieldConditions(draft, field, replacement));
  }

  return (
    <div className="quick-filters">
      <div className="quick-filter-intro">
        <div>
          <strong>Quick measured filters</strong>
          <div className="faint">
            Ranges match recorded values only. Any includes missing values; Unmeasured/Not recorded isolates them.
          </div>
        </div>
      </div>
      {statsReady && VISUAL_BANDS.every((definition) => stats[definition.field]?.recorded_count === 0) && (
        <div className="quick-filter-note">
          Brightness, sharpness and contrast become available after “Analyze photos” finishes.
        </div>
      )}
      <div className="quick-visual-grid">
        {VISUAL_BANDS.map((definition) => {
          const active = activeVisualBand(draft, definition.field);
          const fieldStats = stats[definition.field];
          const noMeasuredValues = !fieldStats || fieldStats.recorded_count === 0;
          return (
            <div className="quick-filter-card" key={definition.field}>
              <div className="quick-filter-card-head">
                <strong>{definition.label}</strong>
                <Availability stats={fieldStats} ready={statsReady} />
              </div>
              <div className="quick-band-buttons" role="group" aria-label={`${definition.label} measured range`}>
                {(["low", "mid", "high"] as const).map((band) => (
                  <button
                    key={band}
                    className={`quick-band ${band}${active === band ? " is-active" : ""}`}
                    aria-pressed={active === band}
                    disabled={disabled || noMeasuredValues}
                    onClick={() => setVisual(definition.field, band)}
                  >
                    <span>{band === "mid" ? "Mid-range" : band[0].toUpperCase() + band.slice(1)}</span>
                    <small>
                      {band === "low" ? `< ${definition.lowUpper}` : band === "high" ? `> ${definition.highLower}` : `${definition.lowUpper}–${definition.highLower}`}
                    </small>
                  </button>
                ))}
              </div>
              <div className="quick-filter-actions">
                <button
                  className={`btn btn-sm${active === "unmeasured" ? " btn-primary" : ""}`}
                  onClick={() => setVisual(definition.field, "unmeasured")}
                  disabled={disabled || !fieldStats || fieldStats.missing_count === 0}
                >
                  Unmeasured
                </button>
                <button
                  className="btn btn-ghost btn-sm"
                  onClick={() => setVisual(definition.field, null)}
                  disabled={disabled || !draft.some((condition) => condition.field === definition.field)}
                >
                  Any
                </button>
              </div>
            </div>
          );
        })}
      </div>
      <div className="quick-range-grid">
        <StandardRangeControl
          field="iso"
          label="ISO"
          stops={STANDARD_RANGE_STOPS.iso}
          stats={stats.iso}
          statsReady={statsReady}
          draft={draft}
          onChange={onChange}
          disabled={disabled}
        />
        <StandardRangeControl
          field="focal_length"
          label="Focal length"
          unit=" mm"
          stops={STANDARD_RANGE_STOPS.focal_length}
          stats={stats.focal_length}
          statsReady={statsReady}
          draft={draft}
          onChange={onChange}
          disabled={disabled}
        />
      </div>
    </div>
  );
}
