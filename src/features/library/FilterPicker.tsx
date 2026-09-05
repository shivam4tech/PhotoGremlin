import { useEffect, useId, useRef, useState } from "react";
import type { FilterCondition } from "@/types/api";
import { choiceIsActive, searchFilterChoices, type FilterChoice } from "./filterDiscovery";

export function FilterPicker({ draft, disabled, onSelect }: {
  draft: FilterCondition[];
  disabled?: boolean;
  onSelect: (choice: FilterChoice) => void;
}) {
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState(false);
  const [index, setIndex] = useState(0);
  const root = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const listId = useId();
  const choices = searchFilterChoices(query);
  useEffect(() => {
    if (!open) return;
    const outside = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    window.addEventListener("pointerdown", outside);
    return () => window.removeEventListener("pointerdown", outside);
  }, [open]);
  useEffect(() => {
    if (open) document.getElementById(`${listId}-${index}`)?.scrollIntoView({ block: "nearest" });
  }, [index, open, listId]);
  function choose(choice: FilterChoice) {
    input.current?.focus(); setQuery(""); setIndex(0); setOpen(false); onSelect(choice);
  }
  return <div className="filter-picker" ref={root} onBlur={(event) => {
    if (!event.currentTarget.contains(event.relatedTarget)) setOpen(false);
  }}>
    <div className="filter-search-row">
      <input ref={input} className="input filter-search" type="search"
        placeholder="Search filters…" aria-label="Search filters" role="combobox"
        aria-expanded={open} aria-controls={listId} aria-autocomplete="list"
        aria-activedescendant={open && choices.length ? `${listId}-${index}` : undefined}
        value={query} disabled={disabled} onFocus={() => setOpen(true)}
        onChange={(event) => { setQuery(event.target.value); setIndex(0); setOpen(true); }}
        onKeyDown={(event) => {
          if (event.key === "Escape") { event.stopPropagation(); setOpen(false); }
          if (event.key === "ArrowDown" || event.key === "ArrowUp") {
            event.preventDefault(); setOpen(true);
            setIndex((current) => open ? Math.max(0, Math.min(choices.length - 1, current + (event.key === "ArrowDown" ? 1 : -1))) : 0);
          }
          if (event.key === "Enter" && open && choices[index]) { event.preventDefault(); choose(choices[index]); }
        }} />
      <button className="btn btn-sm" type="button" disabled={disabled} aria-label="Add filter" aria-expanded={open}
        onClick={() => { input.current?.focus(); setOpen(true); }}>+</button>
    </div>
    {open && <div className="filter-picker-popover">
      <div className="filter-picker-results" id={listId} role="listbox" aria-label="Available filters">
        {choices.map((choice, position) => <div key={choice.id} role="presentation">
          {(position === 0 || choice.category !== choices[position - 1].category) &&
            <div className="filter-picker-category" role="presentation">{choice.category}</div>}
          <div id={`${listId}-${position}`} role="option" aria-selected={position === index}
            className={`filter-picker-option${position === index ? " is-highlighted" : ""}`}
            onPointerMove={() => setIndex(position)} onMouseDown={(event) => event.preventDefault()}
            onClick={() => choose(choice)}>
            <span>{choice.label}</span>{choiceIsActive(choice, draft) && <small>Added</small>}
          </div>
        </div>)}
      </div>
      {!choices.length && <p className="filter-picker-empty" role="status">No filters found. Try “ISO” or “face”.</p>}
      <div className="filter-picker-foot">↑ ↓ Navigate · Enter choose · Esc close</div>
    </div>}
  </div>;
}
