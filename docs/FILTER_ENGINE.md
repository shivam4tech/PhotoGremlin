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

 ## Field registry (Sprint 5, + session_id in Sprint 8)

 | area | fields | source |
 |---|---|---|
 | technical | sharpness, brightness, contrast, saturation, highlight_clipping, shadow_clipping | analysis |
 | visual | monochrome, color (inverse of monochrome), dark, bright | analysis flags |
 | orientation | landscape, portrait, square | photos (w×h) |
 | camera | camera_model, camera_make, lens | photos (EXIF) |
 | exposure | iso, aperture, shutter_speed, focal_length | photos (EXIF) |
 | time | capture_datetime (point, range via `between`) | photos |
 | session | session_id (int) | photos (`= != in is-null not-null`; "open a session in the Library") |
 | local intelligence | faces_present, face_count, smiling, smile_count | analysis (nullable until AI) |
| marking (Sprint 13) | rating (int, null = unrated — `is-null`/`not-null` select unrated/rated), flagged (bool, `photos.flag = 1`), color_label (text, fixed enum) | photos (curatorial marks) |

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

## Implementation (Sprint 5)

- `src-tauri/src/filters/mod.rs` is the engine: pure, Tauri- and
  DB-independent. `parse_filter(json)` validates the JSON (top-level
  operator must be `AND`, ≤ 50 conditions); `build_where(filter)` lowers it
  to a `(WHERE fragment, [SqlParam])` pair. Column names come **only** from a
  compile-time field registry (`FieldDef { kind, expr, negate_bool }`);
  every value is a bound parameter (injection-safe). Unknown fields,
  operators, or value types → friendly `Validation` errors before any SQL.
- Kind rules: `Real`/`Int` accept `= != > >= < <= between in is-null
  not-null`; `DateTime` accepts the order/range ops plus null-ops but not
  `in` (v0.1); `Bool` is `= !=` only; `Text` is `= != in is-null not-null`.
  `in` lists are capped at 100 items.
- Storage semantics: technical fields read `analysis.*` through a
  `LEFT JOIN`, so **unanalyzed photos never match a technical or flag
  condition** (NULL comparison is false — a photo we have not measured is
  neither "sharpness ≥ 70" nor "monochrome"). `color` is stored as the
  inverse of the `is_monochrome` flag. `faces_present` / `smiling` compare
   `(face_count IS NOT NULL AND face_count > 0)` — always false until the
   local-model sprints (9/10) fill those columns. `capture_datetime` is TEXT
   (UTC RFC3339), so comparisons are lexicographic and equal to time order.
   `session_id` (Sprint 8) is an `Int` on `photos.session_id`: scoping a grid
   to one shoot (`= <id>`), to several (`in [..]`), or to unassigned photos
   (`is-null`). It is the engine-level backing for "Open in library" on a
   session (Sessions view) and for saved views that pin a session.
- Execution: `commands/filters.rs::list_filtered_photos` = parse → build →
  `Db::photos_where(where_sql, params, offset, limit)`, which appends the
  stable `ORDER BY` and `LIMIT ? OFFSET ?` and returns a `PhotoPage` (same
  shape as the unfiltered grid — an empty filter is the default path). The
  same `WHERE` will feed `SELECT COUNT(*)` and the statistics engine
  (Sprint 10) for scoped aggregates.
- The UI half (`src/features/library/filterFields.ts` + `FilterBar.tsx`)
  mirrors the registry 1:1 and emits the exact wire object; date pickers send
  bare dates and the upper `between` bound is extended to end-of-day so
  "this day" is inclusive (a visible, stored part of the condition).

## Saved views (Sprint 8)

`saved_views.filter_json` stores the exact filter object. Views are dynamic:
apply the filter at open time — the list adapts to library changes, new
analysis, etc. (Spec: a saved view stores the filter definition, not a static
list.)

- **Validation**: `save_view` parses + builds the filter with the grid's own
  engine *before* persisting, so a stored view can never be one the grid
  cannot evaluate.
- **Dynamic count**: the per-view photograph count is recomputed on demand
  (`photos_where` over the stored filter, 1-row probe) — never stored, so it
  can't go stale.
- **Apply**: the frontend parses `filter_json` back to conditions, loads them
  into the shared library filter, and navigates to the Library. Saving
  overwrites a same-named view (`upsert`, same `id`, `updated_at` moves).
- Names: trimmed, 1–60 chars (frontend `cleanName`), uniqueness enforced in
  the DB.

## Language rules

Filter names and results use neutral technical language: "sharpness ≥ 70",
"ISO below 1600", "contains faces". No "select these", no "delete these".
