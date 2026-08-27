import { useEffect, useState } from "react";
import { api } from "@/lib/ipc";
import type {
  FilterCondition,
  NumericFilterStats,
  QuickNumericFilterField,
} from "@/types/api";
import {
  STANDARD_FILTER_STOPS,
  VISUAL_BANDS,
  activeStandardThreshold,
  activeVisualBand,
  replaceFieldConditions,
  standardThresholdCondition,
  visualBandCondition,
  type ThresholdDirection,
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

interface ThresholdControlProps {
  field: "iso" | "focal_length";
  label: string;
  unit?: string;
  stops: readonly number[];
  defaultValue: number;
  stats?: NumericFilterStats;
  statsReady: boolean;
  draft: FilterCondition[];
  onChange: (conditions: FilterCondition[]) => void;
  disabled?: boolean;
}

function nearestStopIndex(stops: readonly number[], value: number): number {
  return stops.reduce(
    (closest, stop, index) => Math.abs(stop - value) < Math.abs(stops[closest] - value) ? index : closest,
    0,
  );
}

function StandardThresholdControl({
  field,
  label,
  unit = "",
  stops,
  defaultValue,
  stats,
  statsReady,
  draft,
  onChange,
  disabled,
}: ThresholdControlProps) {
  const existing = draft.find((condition) => condition.field === field);
  const existingThreshold = activeStandardThreshold(draft, field);
  const [direction, setDirection] = useState<ThresholdDirection>(existingThreshold?.direction ?? "up-to");
  const [stopIndex, setStopIndex] = useState(() => nearestStopIndex(stops, existingThreshold?.value ?? defaultValue));

  useEffect(() => {
    if (!existingThreshold) return;
    setDirection(existingThreshold.direction);
    setStopIndex(nearestStopIndex(stops, existingThreshold.value));
  }, [existingThreshold?.direction, existingThreshold?.value, stops]);

  const notRecorded = existing?.operator === "is-null";
  const noRecordedValues = !stats || stats.recorded_count === 0;
  const currentValue = stops[stopIndex];

  function applyThreshold(nextDirection: ThresholdDirection, nextIndex: number) {
    onChange(replaceFieldConditions(
      draft,
      field,
      standardThresholdCondition(field, nextDirection, stops[nextIndex]),
    ));
  }

  return (
    <div className="quick-filter-row quick-filter-threshold-row">
      <div className="quick-filter-label">
        <strong>{label}</strong>
        <Availability stats={stats} noun="recorded" missingNoun="not recorded" ready={statsReady} />
      </div>
      <div className="quick-threshold-control">
        <div className="quick-direction" role="group" aria-label={`${label} threshold direction`}>
          {(["up-to", "from"] as const).map((nextDirection) => (
            <button
              type="button"
              key={nextDirection}
              className={direction === nextDirection && existingThreshold ? "is-active" : ""}
              aria-pressed={direction === nextDirection && existingThreshold !== null}
              disabled={disabled || noRecordedValues}
              onClick={() => {
                setDirection(nextDirection);
                applyThreshold(nextDirection, stopIndex);
              }}
            >
              {nextDirection === "up-to" ? "Up to" : "From"}
            </button>
          ))}
        </div>
        <input
          type="range"
          min={0}
          max={stops.length - 1}
          value={stopIndex}
          disabled={disabled || noRecordedValues}
          aria-label={`${label} threshold`}
          aria-valuetext={`${direction === "up-to" ? "Up to" : "From"} ${currentValue.toLocaleString()}${unit}`}
          onChange={(event) => {
            const nextIndex = Number(event.target.value);
            setStopIndex(nextIndex);
            applyThreshold(direction, nextIndex);
          }}
        />
        <output className="quick-threshold-value mono">
          {currentValue.toLocaleString()}{unit}
        </output>
      </div>
      <div className="quick-row-actions">
        <button
          type="button"
          className={`quick-missing${notRecorded ? " is-active" : ""}`}
          onClick={() => onChange(replaceFieldConditions(draft, field, { field, operator: "is-null", value: null }))}
          disabled={disabled || !stats || stats.missing_count === 0}
        >
          Not recorded
        </button>
        <button
          type="button"
          className="quick-clear"
          onClick={() => onChange(replaceFieldConditions(draft, field, null))}
          disabled={disabled || !existing}
          aria-label={`Clear ${label} filter`}
          title={`Clear ${label} filter and include any value`}
        >
          ×
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
        <strong>Quick filters</strong>
        <span className="faint">Local measurements only · clear a row to include any value</span>
      </div>
      {statsReady && VISUAL_BANDS.every((definition) => stats[definition.field]?.recorded_count === 0) && (
        <div className="quick-filter-note">
          Brightness, sharpness and contrast become available after “Analyze photos” finishes.
        </div>
      )}
      <div className="quick-filter-rows">
        {VISUAL_BANDS.map((definition) => {
          const active = activeVisualBand(draft, definition.field);
          const fieldStats = stats[definition.field];
          const noMeasuredValues = !fieldStats || fieldStats.recorded_count === 0;
          return (
            <div className="quick-filter-row" key={definition.field}>
              <div className="quick-filter-label">
                <strong>{definition.label}</strong>
                <Availability stats={fieldStats} ready={statsReady} />
              </div>
              <div className="quick-band-buttons" role="group" aria-label={`${definition.label} measured category`}>
                {(["low", "mid", "high"] as const).map((band) => (
                  <button
                    type="button"
                    key={band}
                    className={`quick-band${active === band ? " is-active" : ""}`}
                    aria-pressed={active === band}
                    disabled={disabled || noMeasuredValues}
                    onClick={() => setVisual(definition.field, band)}
                    title={`${definition.label} ${band === "low" ? `below ${definition.lowUpper}` : band === "high" ? `above ${definition.highLower}` : `${definition.lowUpper} to ${definition.highLower}`}`}
                  >
                    <span>{band === "mid" ? "Mid" : band[0].toUpperCase() + band.slice(1)}</span>
                    <small>
                      {band === "low" ? `< ${definition.lowUpper}` : band === "high" ? `> ${definition.highLower}` : `${definition.lowUpper}–${definition.highLower}`}
                    </small>
                  </button>
                ))}
              </div>
              <div className="quick-row-actions">
                <button
                  type="button"
                  className={`quick-missing${active === "unmeasured" ? " is-active" : ""}`}
                  onClick={() => setVisual(definition.field, "unmeasured")}
                  disabled={disabled || !fieldStats || fieldStats.missing_count === 0}
                >
                  Unmeasured
                </button>
                <button
                  type="button"
                  className="quick-clear"
                  onClick={() => setVisual(definition.field, null)}
                  disabled={disabled || !draft.some((condition) => condition.field === definition.field)}
                  aria-label={`Clear ${definition.label} filter`}
                  title={`Clear ${definition.label} filter and include any value`}
                >
                  ×
                </button>
              </div>
            </div>
          );
        })}
        <StandardThresholdControl
          field="iso"
          label="ISO"
          stops={STANDARD_FILTER_STOPS.iso}
          defaultValue={1600}
          stats={stats.iso}
          statsReady={statsReady}
          draft={draft}
          onChange={onChange}
          disabled={disabled}
        />
        <StandardThresholdControl
          field="focal_length"
          label="Focal length"
          unit=" mm"
          stops={STANDARD_FILTER_STOPS.focal_length}
          defaultValue={85}
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
