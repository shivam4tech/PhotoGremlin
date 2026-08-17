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
