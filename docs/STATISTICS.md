# Statistics

The statistics engine is a **UI-independent service** (Rust, Sprint 6) that
answers aggregation queries against `photos` + `analysis`. The dashboard,
session pages, comparisons and future reports all call the same engine —
there is one implementation of "average sharpness", not one per screen.

## Time periods

One period model, no per-period special cases:

```
period := today | this-week | this-month | this-year | custom(from, to) | all
```

Each resolves to a `capture_datetime` range in UTC; sessions/photos without a
capture datetime fall back to `indexed_at`.

## Core queries (Sprint 6)

- totals: photos, sessions, photos/session
- averages: sharpness, brightness, contrast, saturation (analyzed rows only —
  unanalyzed photos are excluded, and the UI says so)
- shares: monochrome %, color %, face-present %, smiling %
- EXIF distributions (histograms, fixed bins):
  - ISO: 100–400 · 400–800 · 800–1600 · 1600–3200 · 3200+
  - aperture: f/1.4–2.0 · f/2.0–2.8 · f/2.8–4.0 · f/4–8 · f/8+
  - focal length: 24 · 35 · 50 · 85 · 135 mm buckets
  - shutter: 1s+ · 1/2–1 · 1/30–1/2 · 1/125–1/30 · 1/1000–1/125 · 1/4000–1/1000 · 1/8000+
- usage counts: camera usage, lens usage (photos + % + avg sharpness/ISO)
- trends over time (monthly buckets): avg sharpness, avg ISO, sessions/month,
  photos/month, color vs mono share, lens usage over time
- per-session summary: everything above scoped to one session + duration
- session comparison: N sessions side by side on the same metric rows
- camera comparison, lens comparison: usage-based tables (never ranked as
  "better")
- selection infrastructure: imported vs selected vs trashed counts from
  file_operations + selection state; ratio = selected/imported (only when a
  selection signal exists)

## Honest-data rules

- A metric only renders when its inputs exist. "0 analyzed photos → sharpness
  unavailable", not "sharpness 0".
- Averages report the analyzed subset, with the denominator shown.
- No fabricated history: trends only include periods with data.

## Language discipline (spec §50)

Correct: "Your average sharpness increased from 78 to 84."
Wrong: "Your photography improved."

Correct: "You used the 85mm lens in 32% of photographs."
Wrong: "85mm is your best lens."

The engine emits numbers and labels; phrasing lives in the UI copy and follows
this rule. There is no "better/worse" in the data layer at all.

## Implementation (`src-tauri/src/statistics/`, Sprint 6)

`statistics/mod.rs` is the engine (Tauri-independent; takes `&Db`),
`statistics/bins.rs` holds the pure binning functions. Entry points:

- `period_stats(db, &Period, now)` — dashboard scope. `Period` is the one
  JSON model (`{"kind":"today" | "this-week" | "this-month" | "this-year" |
  "all"}`, or `{"kind":"custom","from","to"}`); `now` is injected (`Utc::now()`
  in the command) so resolution is unit-testable.
- `session_summary(db, session_id)` — the same core scoped to one session,
  plus shoot `duration_days` (min→max of the photos' best-known times).
- `compare_sessions(db, ids)` — 1–8 sessions on the same metric rows.

### Period resolution

Each period resolves at `now` to RFC3339 UTC **string** bounds
(`now` is pinned second-precision): Today = midnight→midnight; This week =
**Monday-based** (ISO) midnight→next Monday; This month / This year =
calendar edges; All = unbounded; Custom = `[from 00:00Z, to 23:59:59Z]` when
the dates are bare `YYYY-MM-DD` (end-of-day extension), full RFC3339 bounds
otherwise. Calendar edges are exclusive (`<`; stored times are whole seconds
so no photo is lost); the custom upper bound is inclusive (`<=`) because it
reaches the day's last stored second. The query never compares datetimes —
it compares the catalog's stored strings, which are UTC RFC3339, so
lexicographic == chronological.

**Time source:** every scope filters on
`COALESCE(p.capture_datetime, p.indexed_at)` — photos without a capture
datetime fall back to their index time (documented in the period model).

### Honest-data mapping (type-level)

- Averages and mono/color shares come from the `analysis` JOIN only: zero
  analyzed rows → SQL `AVG`/`SUM` are NULL → `None` in the struct → the UI
  renders "unavailable", never 0. `analyzed` (the denominator) ships with
  every result.
- Face/smile shares are computed over the subset where the AI column is
  non-NULL; when no photo in scope carries AI data, both are `None`.
- The histogram bins and usage tables come from the raw EXIF columns in
  scope (NULLs simply never enter a bin); an all-NULL scope renders stable
  zero bins, and the UI says "No data in this period".
- The trend is a `GROUP BY` on the month of the scoped time source — only
  months with data exist — returned oldest-first, capped at the most recent
  36 months.

### Distributions (fixed bins, `bins.rs`, edges pinned in unit tests)

Bins are half-open `[lo, hi)` except the last of each family, which is
closed; values below the family's first lower bound clip into the first bin
(ISO 50 → "100–400"; f/1.2 → "f/1.4–2.0"). Documented edge: `iso_bin(400) =
"400–800"`. Focal length assigns to the **nearest** of 24/35/50/85/135 mm
(60 → "50 mm", 72 → "85 mm"). Shutter labels follow the specification
verbatim; "1/8000+" is the overflow bucket (< 1/4000 s). Columns are fetched
in scope and binned in Rust (a few MB even for huge libraries), keeping the
SQL simple and the binning unit-testable.

### Usage, trend, selection

- `camera_usage` / `lens_usage`: `GROUP BY` the (trimmed, NULL→"Unknown
  camera"/"Unknown lens") name, top 20 by count, with share of the scope and
  analyzed-only `AVG(sharpness)` / `AVG(iso)`. No ordering of quality.
- Trend row: month, photos, distinct sessions, avg sharpness (analyzed),
  avg ISO, color share (analyzed non-mono ÷ analyzed).
- Selection: `imported` = photos in scope; `selected`/`rejected` from
  `selections` scoped to the period; `trashed` = distinct trashed paths in
  `file_operations` (global — ops pre-date period scoping). The whole section
  is present **iff** a selection signal exists (any scoped selection state,
  or any move/copy/rename/trash operation); `kept_ratio = selected ÷
  imported` only when `imported > 0`.

### IPC + UI

`commands/stats.rs` exposes three synchronous pure-SQL commands:
`period_stats(periodJson)`, `session_summary(sessionId)`,
`compare_sessions(sessionIds)`. The TS mirrors live in
`src/types/api.ts` (`PeriodStats`, `SessionSummary`, `SessionMetrics`,
`BinCount`, `UsageCount`, `TrendPoint`, `SelectionStats`); formatting and the
"unavailable" rendering are pure functions in `src/features/stats/format.ts`.
Session shoot periods (`sessions.start_time/end_time`) are maintained by
`refresh_session_counts` (per-scan) and `refresh_all_sessions_times`
(post-metadata-pass) — see DATABASE.md.
