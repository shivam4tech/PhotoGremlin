import { useEffect, useState } from "react";
import type { FilterCondition } from "@/types/api";
import {
  AREA_ORDER,
  FILTER_FIELDS,
  FIELD_BY_NAME,
  OPS_BY_KIND,
  buildCondition,
  chipLabel,
} from "./filterFields";

interface FilterBarProps {
  draft: FilterCondition[];
  onChange: (conditions: FilterCondition[]) => void;
  disabled?: boolean;
}

/**
 * The library's active filter, edited as structured conditions (not UI
 * state): what's rendered here is exactly the object sent to the Rust
 * engine and (later) stored in saved views. Neutral technical language
 * throughout (FILTER_ENGINE.md).
 */
export function FilterBar({ draft, onChange, disabled }: FilterBarProps) {
  const [open, setOpen] = useState(draft.length > 0);
  const [field, setField] = useState(FILTER_FIELDS[0].field);
  const def = FIELD_BY_NAME[field];
  const ops = OPS_BY_KIND[def.kind];
  const [op, setOp] = useState<FilterCondition["operator"]>(ops[0].op);
  const [raw, setRaw] = useState("");
  const [raw2, setRaw2] = useState("");

  // Keep the operator valid when the field's kind changes.
  useEffect(() => {
    const allowed = OPS_BY_KIND[FIELD_BY_NAME[field].kind];
    if (!allowed.some((o) => o.op === op)) setOp(allowed[0].op);
  }, [field, op]);

  const needsTwoValues = op === "between" && def.kind !== "datetime";
  const needsValue = !["is-null", "not-null"].includes(op);
  const candidate = needsValue
    ? buildCondition(field, op, raw, raw2)
    : buildCondition(field, op, raw, raw2);
  const canAdd = candidate !== null;

  function selectField(f: string) {
    setField(f);
    const first = OPS_BY_KIND[FIELD_BY_NAME[f].kind][0].op;
    setOp(first);
    setRaw("");
    setRaw2("");
  }

  function add() {
    if (!candidate) return;
    onChange([...draft, candidate]);
    setRaw("");
    setRaw2("");
  }

  function remove(i: number) {
    onChange(draft.filter((_, j) => j !== i));
  }

  function valueInput() {
    if (!needsValue) return null;
    switch (def.kind) {
      case "bool":
        return (
          <select
            className="input"
            value={raw === "false" ? "false" : "true"}
            onChange={(e) => setRaw(e.target.value)}
          >
            <option value="true">true</option>
            <option value="false">false</option>
          </select>
        );
      case "text":
        if (def.values) {
          return (
            <select className="input" value={raw} onChange={(e) => setRaw(e.target.value)}>
              <option value="">—</option>
              {def.values.map((v) => (
                <option key={v} value={v}>
                  {v}
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
          />
        );
      case "datetime":
        if (op === "between") {
          return (
            <>
              <input
                className="input"
                type="date"
                aria-label="from date"
                value={raw}
                onChange={(e) => setRaw(e.target.value)}
              />
              <span className="faint">→</span>
              <input
                className="input"
                type="date"
                aria-label="to date"
                value={raw2}
                onChange={(e) => setRaw2(e.target.value)}
              />
            </>
          );
        }
        return (
          <input
            className="input"
            type="date"
            value={raw}
            onChange={(e) => setRaw(e.target.value)}
          />
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
                aria-label="minimum"
                value={raw}
                onChange={(e) => setRaw(e.target.value)}
              />
              <span className="faint">→</span>
              <input
                className="input"
                type="number"
                step={def.kind === "int" ? 1 : "any"}
                aria-label="maximum"
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
          />
        );
    }
  }

  return (
    <div className="filterbar">
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

      {open && (
        <div className="filterbar-panel">
          {draft.length > 0 && (
            <div className="filterbar-chips">
              {draft.map((c, i) => (
                <span key={`${c.field}-${i}`} className="chip">
                  {chipLabel(c)}
                  <button
                    className="chip-x"
                    onClick={() => remove(i)}
                    aria-label={`remove filter ${chipLabel(c)}`}
                  >
                    ×
                  </button>
                </span>
              ))}
              <button className="btn btn-ghost btn-sm" onClick={() => onChange([])}>
                Clear all
              </button>
            </div>
          )}

          <div className="filterbar-compose">
            <select className="input" value={field} onChange={(e) => selectField(e.target.value)}>
              {AREA_ORDER.map((area) => (
                <optgroup key={area} label={area}>
                  {FILTER_FIELDS.filter((f) => f.area === area).map((f) => (
                    <option key={f.field} value={f.field}>
                      {f.label}
                    </option>
                  ))}
                </optgroup>
              ))}
            </select>
            <select
              className="input"
              value={op}
              onChange={(e) => setOp(e.target.value as FilterCondition["operator"])}
            >
              {ops.map((o) => (
                <option key={o.op} value={o.op}>
                  {o.label}
                </option>
              ))}
            </select>
            {valueInput()}
            <button className="btn btn-sm" onClick={add} disabled={!canAdd || disabled}>
              Add
            </button>
          </div>
          {needsTwoValues && (
            <div className="faint mono" style={{ fontSize: 11 }}>
              between: two values, min → max
            </div>
          )}
        </div>
      )}
    </div>
  );
}
