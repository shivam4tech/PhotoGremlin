# Filter Engine

Filters are **structured data**, not UI state. One representation is shared by
the library grid, saved views, collections (as starting points), and the
statistics engine — the engine is never tied to a React component.

## Wire format (JSON)

```json
{
  "operator": "AND",
  "conditions": [
    { "field": "sharpness",   "operator": ">=", "value": 70 },
    { "field": "orientation", "operator": "=",   "value": "portrait" },
    { "field": "iso",         "operator": "<",   "value": 1600 }
  ]
}
```

Top-level operator is AND in v0.1 (composability comes from many conditions).
`conditions` are evaluated against `photos` joined with `analysis` where the
field lives.

## Operators

`=`, `!=`, `>`, `>=`, `<`, `<=`, `between` (value = `[lo, hi]`),
`in` (value = array), `is-null`, `not-null`.

## Field registry (Sprint 5)

| area | fields | source |
|---|---|---|
| technical | sharpness, brightness, contrast, saturation, highlight_clipping, shadow_clipping | analysis |
| visual | monochrome, color (inverse of monochrome), dark, bright | analysis flags |
| orientation | landscape, portrait, square | photos (w×h) |
| camera | camera_model, camera_make, lens | photos (EXIF) |
| exposure | iso, aperture, shutter_speed, focal_length | photos (EXIF) |
| time | capture_datetime (point, range via `between`) | photos |
| local intelligence | faces_present, face_count, smiling, smile_count | analysis (nullable until AI) |

The registry maps each field to (table, column, type, comparator) so
conditions validate before hitting SQL, and unknown fields fail with a
friendly error instead of a SQL error.

## Execution

1. Parse + validate filter JSON (serde, in `filters` logic on the Rust side).
2. Translate conditions to a parameterized SQL `WHERE` (never string-built
   user input — values are always bound parameters).
3. Return paginated photo rows for the grid; the same WHERE feeds
   `SELECT COUNT(*)` for the results count and the statistics engine for
   scoped aggregates.

## Saved views

`saved_views.filter_json` stores the exact filter object. Views are dynamic:
apply the filter at open time — the list adapts to library changes, new
analysis, etc. (Spec: a saved view stores the filter definition, not a static
list.)

## Language rules

Filter names and results use neutral technical language: "sharpness ≥ 70",
"ISO below 1600", "contains faces". No "select these", no "delete these".
