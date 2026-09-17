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
 | visual | monochrome, color (inverse of monochrome), dark, bright, palette_color | analysis flags + local 12-bit hue signature |
 | orientation | landscape, portrait, square | photos (w×h) |
| camera | camera_model, camera_make, lens | photos (EXIF) |
 | exposure | iso, aperture, shutter_speed, focal_length | photos (EXIF) |
 | time | capture_datetime (point, range via `between`) | photos |
 | session | session_id (int) | photos (`= != in is-null not-null`; "open a session in the Library") |
 | local intelligence | faces_present, face_count, closed_eye_candidate, eye_closure_confidence, possible_blink | analysis (nullable until the relevant local pass) |
| scene (Sprint 18) | scene_group (`analysis.scene_coarse`, the MERGED product chip from the local scene model; NULL until the pass runs) | analysis |
| marking (Sprint 13) | rating (int, null = unrated — `is-null`/`not-null` select unrated/rated), flagged (bool, `photos.flag = 1`), color_label (text, fixed enum) | photos (curatorial marks) |
| review (Sprint 19) | review_state (`selected`, `rejected`, `needs_attention`; `is-null` = unreviewed) | correlated local `selections` row |

The registry maps each field to (table, column, type, comparator) so
conditions validate before hitting SQL, and unknown fields fail with a
friendly error instead of a SQL error.

### Metadata value pickers

When the Library is scoped to a shoot, the camera make, camera model, and
lens fields load their distinct **local** EXIF values with counts. The picker
also offers `Unidentified (n)`, which writes the normal `is-null` condition
(blank EXIF strings count as unidentified). This is a convenience layer over
the same filter wire format — no value is inferred, sent to a service, or
embedded into SQL.

### Quick measured controls (Sprints 22–23, revamped Sprints 25–26 and filter controls Sprint 2)

The Library presents common numeric filters without requiring photographers
to compose raw operators:

- brightness, sharpness, contrast, saturation, highlight clipping, shadow clipping and
  eye-closure confidence use inclusive ranges from 0–100;
- ISO and focal length use the existing increasing photographic stop lists;
- leaving both handles at the full domain removes that field, moving only the
  lower handle writes `>=`, moving only the upper handle writes `<=`, and
  moving both writes `between`.

Measured rows show their current condition without opening a parent accordion.
Each row expands independently, revealing keyboard-accessible inset range thumbs
and editable minimum/maximum numbers. A selected range is highlighted and the
active thumb shows its value while dragging. The slider uses photographic stops
for ISO and focal length, while typed values retain their exact number. Choosing
**Exact value** switches to a single number. Pointer release, keyboard release,
or blur commits the draft range, avoiding a stream of SQLite queries during a
drag. The same range control is used in the Advanced editor, where changes stay
temporary until **Add filter** or **Save change**. Search still exposes fields
whose engine conditions need more specific operators.

Every control reports locally recorded counts from `numeric_filter_stats`,
including missing counts only when nonzero. Saturation is included in this
statistics allowlist. A numeric range follows SQL NULL semantics
and therefore excludes unmeasured photographs. `Unmeasured` (visual analysis)
or `Not recorded` (EXIF) writes `is-null`; `Reset to any` removes that field's
quick/advanced condition and includes both known and missing values. Missing
values are never guessed from filenames, pixels, or neighboring photographs.

Saved views keep the ordinary filter wire format. Legacy strict `<`/`>` quick
conditions and arbitrary advanced numeric conditions load without being
rewritten. Strict bounds and values outside the control domain disable the
scrubber and keep the original condition visible. All conditions remain visible and removable in the sticky
active-filter list, even when their measured row is collapsed.

Boolean editors use named segmented choices, rating uses stars, and enum and
shoot-specific metadata values use a searchable keyboard-operated picker. These
controls continue to emit the existing structured condition wire format.

### Color explorer (Sprint 35)

The inspector's compact grid of twelve labelled hue swatches writes one ordinary
`palette_color in [...]` condition. Multiple selected hues use **match any**
semantics inside that condition; the condition remains AND-composed with
rating, camera, date, review-state, and measured filters. Selecting no hues
removes the condition. This makes color exploration immediately reversible
and means saved views persist the exact same portable filter definition as the
grid.

Rust lowers the selected hue names to a bound integer mask and evaluates
`(analysis.color_signature & ?) != 0`. Only the twelve compile-time names are
accepted and the list is capped at twelve. The signature is calculated from
pixels by the deterministic local analysis pass (IMAGE_ANALYSIS.md), not from
labels, a remote service, or an aesthetic model. An unanalyzed photograph has
no matching measured hue until analysis completes.

### Quick filters (Sprints 32–34)

The Library inspector places one-click presets above the measured controls.
They are presented as a compact two-column grid with borderless rows; checked
state and the selected background communicate activation without table lines.
Sprint 33 adds **Potentially soft** (`sharpness < 40`), **Highlight clipping**
and **Shadow clipping** (each ≥ 5%), and **Closed-eye candidate** to the
existing color, brightness, orientation and face-presence shortcuts. They are
ordinary structured conditions and introduce no alternate query path or
aesthetic verdict. The first three use deterministic measurements; eye state
uses the optional local models only when such measurements exist.

Sprint 34 adds **Possible blink**, backed by the nullable contextual
`analysis.possible_blink` field. The preset and advanced Boolean filter match
only explicit `true`. An explicit `false` means a complete adjacent-frame
window was evaluated without the pattern; `NULL` means the burst or local eye
context was insufficient. SQL keeps those states distinct, so neither a true
nor false filter silently includes unknown photographs.

Selecting an active preset again removes only that preset. Mutually exclusive
pairs replace one another (black-and-white/color, dark/bright, and
landscape/portrait), while unrelated conditions such as ISO, lens, review
state, or sharpness remain intact. The buttons report pressed state and become
unavailable while the Library is not filterable.

The complete filter surface lives in the Library's collapsible right inspector.
Its header reports live results and Clear all; the sticky discovery area keeps
active chips and filter search accessible while the controls scroll. Search
includes presets and every registered field, with Arrow/Enter/Escape support
and Added indicators. Selecting a field opens the existing typed operator/value
composer, not a native field dropdown. Sidebar changes apply live; there is no
staging or Apply step. Save as view becomes available when conditions exist.
The left rail's `Unreviewed`, `Kept`, and
`Needs attention` shortcuts replace only the `review_state` condition, so a
photographer can change review state while retaining ISO, lens, brightness, or
other active conditions. Selecting the active review shortcut again clears
only `review_state`.

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
  `in` lists are capped at 100 items. The bespoke `Palette` kind accepts only
  `in`, validates the 12 hue names, and binds the resulting bitmask.
- Storage semantics: technical fields read `analysis.*` through a
  `LEFT JOIN`, so **unanalyzed photos never match a technical or flag
  condition** (NULL comparison is false — a photo we have not measured is
  neither "sharpness ≥ 70" nor "monochrome"). `color` is stored as the
  inverse of the `is_monochrome` flag. `faces_present` compares
   `(face_count IS NOT NULL AND face_count > 0)` — NULL stays distinct from a
   completed result of zero. `capture_datetime` is TEXT
   (UTC RFC3339), so comparisons are lexicographic and equal to time order.
   `closed_eye_candidate` is true only when `closed_eye_face_count > 0`;
   unavailable eye analysis never matches it. `eye_closure_confidence` is the
   nullable 0–100 aggregate stored by the eye pass. `session_id` (Sprint 8) is
   an `Int` on `photos.session_id`: scoping a grid
   to one shoot (`= <id>`), to several (`in [..]`), or to unassigned photos
   (`is-null`). It is the engine-level backing for "Open in library" on a
   session (Sessions view) and for saved views that pin a session.
- `review_state` intentionally uses a correlated `selections` lookup rather
  than widening every grid query with another join. `is-null` means no
  decision row exists yet — the useful **unreviewed** queue; all other values
  are explicit photographer decisions and can be combined with session scope.
- Execution: `commands/filters.rs::list_filtered_photos` = parse → build →
  `Db::photos_where(where_sql, params, offset, limit)`, which appends the
  stable `ORDER BY` and `LIMIT ? OFFSET ?` and returns a `PhotoPage` (same
  shape as the unfiltered grid — an empty filter is the default path). The
  same `WHERE` will feed `SELECT COUNT(*)` and the statistics engine
  (Sprint 10) for scoped aggregates.
- The UI half (`src/features/library/filterFields.ts` + `FilterBar.tsx`)
  exposes the supported user-facing registry and emits the exact wire object;
  date pickers send
  bare dates and the upper `between` bound is extended to end-of-day so
  "this day" is inclusive (a visible, stored part of the condition).
- `filterDiscovery.ts`, `FilterPicker.tsx` and `ActiveFilterList.tsx` are
  presentation adapters over that registry and the existing presets. Removing
  a hue chip updates only that hue; removing other chips removes that condition.
- The database and Rust registry retain the reserved `smile_count` / `smiling`
  fields for schema and saved-filter compatibility. They are intentionally not
  offered by the UI because the current release has no local smile model and
  therefore writes no smile results. See `LOCAL_AI.md`.
- `useFilteredPhotos.ts` keeps the current grid visible during refresh and
  ignores stale request completions. Pagination is guarded against duplicate
  requests; a reload replaces page zero instead of appending stale pages.
- Sidebar edits remain live and retain Quick Filters. `AdvancedFiltersDialog.tsx`
  reuses the same controls with a private condition snapshot. The Advanced
  workspace has five registry-backed categories and places palette colors in
  Appearance only. Its reducer owns the selected category and temporary editor:
  switching categories clears an unfinished editor while staged conditions
  remain available as a compact summary. Advanced has no search picker; its
  category rows open temporary editors directly beneath the selected row.
  Staged conditions navigate to their category and open beside their matching
  row. Rating's precise options open beneath its stars, and measured condition
  editors open beneath their measured row. The Simple sidebar retains search.
  Apply replaces the shared store's conditions once; Cancel/Escape discard the
  snapshot. No query, operator, analysis or persistence semantics change.

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
