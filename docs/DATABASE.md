# Database

SQLite, single file: `<data_dir>/database.sqlite` (see DEVELOPMENT.md for
per-OS locations). WAL mode, `PRAGMA foreign_keys=ON`, one
`Mutex<Connection>` in `src-tauri/src/database.rs`.

## Migration policy

Version stored in `schema_version (version, applied_at)`. Migrations are
idempotent batches applied at startup up to `CURRENT_SCHEMA_VERSION`
(currently 11). Tests assert both expected-table presence and idempotency.

- v1: core tables (sessions, photos, analysis, app_settings)
- v2: collections
- v3: saved_views + similarity groups
- v4: file_operations audit log
- v5: partial unique index `sessions(root_path) WHERE root_path IS NOT NULL`
  — one session per imported folder (manual sessions keep `root_path` NULL)
- v6: `analysis.source_mtime TEXT` (NULL on pre-v6 rows) — the source file
  mtime each analysis row was computed from (idempotency check, see below);
  added via `ALTER TABLE`, guarded by a `PRAGMA table_info` probe
- v7: `photos.exif_at TEXT` (NULL until read) — the RFC3339 time the
  metadata (EXIF) pass last read a file. Drives the "metadata pending" count
  and makes re-runs cheap (one read per file in v0.1); added via
  `ALTER TABLE`, guarded by the same `PRAGMA table_info` helper as v6
- v8: `selections` table — explicit culling state (one row per photo:
  `selected` | `rejected`) the statistics engine reads for the selection ratio
  (written by the Sprint 7 culling UI via `set_selections`/`clear_selections`)
- v9: `photos.phash INTEGER` + `photos.phash_source_mtime TEXT` — the
  64-bit dHash and the RFC3339 file mtime it was computed from (the
  similarity pass's incremental rule, mirroring the analysis rule in terms of
  a separate column; see SIMILARITY.md). Added via `ALTER TABLE`, guarded by
  the same `PRAGMA table_info` probe as v6/v7
- v10: `analysis.faces_at TEXT` (NULL until the local-AI face pass stamps
  it) — the file mtime the `face_count` was computed from, driving the face
  pass's incremental rule (re-detect when the file gets newer)
- v10: `analysis.faces_at TEXT` (NULL until the local-AI face pass stamps
  it) — the file mtime the `face_count` was computed from, driving the face
  pass's incremental rule exactly like `phash_source_mtime` does (see
  LOCAL_AI.md). Added via `ALTER TABLE`, guarded by the same
  `PRAGMA table_info` probe
- v11 (Sprint 11): `photos.lens_make TEXT`, `photos.software TEXT`,
  `photos.metadata_source TEXT NOT NULL DEFAULT 'none'` — two further EXIF
  fields, and the provenance column recording where a photo's
  camera/exposure/date values came from (`'none'` → `'exif'` once real EXIF
  lands; the date-estimation sprint adds `'filename'`/`'mtime'` below
  `'exif'` in the dominance order). Also makes the metadata queue
  **incremental**: `exif_at` is stamped per read and a file whose mtime is
  newer than its last read (`file_mtime > exif_at`) is re-read, mirroring
  the v6 analysis rule. Added via `ALTER TABLE`, guarded by the same
  `PRAGMA table_info` probe as v6/v7

## Tables (schema v1–v10)

### sessions
A shoot or imported body of work.

| column | type | notes |
|---|---|---|
| id | INTEGER PK | |
| name | TEXT | human name (e.g. folder name) |
| root_path | TEXT | the scanned folder, if any |
| start_time / end_time | TEXT | UTC RFC3339 — the shoot period, derived from the photos' `COALESCE(capture_datetime, indexed_at)` (NULLs while the session has no dated photos) |
| photo_count | INTEGER | denormalized counter, refreshed by the scan (per session) and the metadata pass (all sessions) |
| created_at | TEXT | |

### photos
One row per indexed file. `path` is UNIQUE (duplicate-path protection:
re-scan upserts instead of duplicating).

| column | type | notes |
|---|---|---|
| id | INTEGER PK | |
| path | TEXT UNIQUE | absolute path |
| filename / extension | TEXT | |
| size_bytes | INTEGER | from `stat` |
| width / height | INTEGER | pixel dimensions (EXIF-orientation-corrected when available) |
| orientation | TEXT | `landscape` \| `portrait` \| `square`, derived from w×h |
| camera_make / camera_model / lens | TEXT | EXIF, filled by the metadata pass (Sprint 5) |
| lens_make | TEXT | (v11) EXIF lens manufacturer — often absent; the model string is the useful one |
| software | TEXT | (v11) EXIF software that created/edited the file (e.g. "Adobe Lightroom") |
| focal_length | REAL | mm (1/100 mm EXIF tag converted to mm) |
| iso | INTEGER | |
| aperture | REAL | f-number, e.g. 2.8 |
| shutter_speed | REAL | seconds (e.g. 1/250 = 0.004) |
| capture_datetime | TEXT | UTC RFC3339 — **best-known capture time**: real EXIF wins, otherwise the estimate (filename → mtime fallback, Sprint 12). EXIF stores a zone-less local clock time; PhotoGremlin stores that wall-clock time **verbatim as UTC** (a documented decision — the catalog stays lexicographically sortable for filters without silently assuming the user's timezone) |
| capture_datetime_source | TEXT | (v12) provenance of the stored date: `'exif'` \| `'filename'` \| `'mtime'`. Estimates are always labelled, never silently merged into "real" dates |
| gps_present | INTEGER | 0/1 — presence only. **Coordinates are never stored** (see PRIVACY.md) |
| session_id | INTEGER → sessions | ON DELETE SET NULL |
| indexed_at | TEXT | |
| file_mtime | TEXT | file mtime; incremental analysis reuses rows when mtime+size unchanged |
| exif_at | TEXT | (v7) RFC3339 time the metadata pass last read this file; NULL = not yet read. Since v11 the queue is incremental: `file_mtime > exif_at` re-reads changed files |
| metadata_source | TEXT | (v11) `'none'` default; `'exif'` once real EXIF values land (`'filename'` / `'mtime'` arrive with date estimation). Dominance order: exif > filename > mtime |
| phash | INTEGER | (v9) 64-bit dHash of the decoded image, from the similarity pass (Sprint 8); NULL = not yet hashed |
| phash_source_mtime | TEXT | (v9) RFC3339 file mtime the hash was computed from; drives the re-hash rule |

Indexes: `session_id`, `capture_datetime`, `camera_model`, `lens` (filter +
dashboard hot paths).

**Upsert semantics (scanner, Sprint 2):** the scanner upserts by `path`.
Re-scans refresh `size_bytes`/`file_mtime`/`session_id`, merge dimensions with
`COALESCE` (a scan that can't read pixels never blanks values a later EXIF
pass filled), and preserve the original `indexed_at`. `upsert_session` is
keyed on `root_path`: re-scanning a folder keeps the same session and
refreshes its name; `refresh_session_counts` re-derives `photo_count` after
each scan pass. Rows for files that vanished from disk are **not** deleted
silently — they stay until a future reconcile step flags them to the user.

**Metadata (EXIF) merge (Sprint 5, incremental since v11/Sprint 11):** the
metadata pass (`exif_queue` → `upsert_exif`) reads each file, stamps
`exif_at`, and re-reads any file whose mtime is newer than its last read
(same incremental rule as analysis — a re-exported/edited file's metadata
stays truthful). `status().metadata_pending` counts the same queue (never
read ∪ changed since read) and drives the UI. `upsert_exif` merges with
`COALESCE`: scanner-resolved dimensions/orientation win (the scanner has
authoritative pixels); EXIF-owned columns are *refreshed by the newest read*
(a non-None value replaces the old one, while an empty read never erases
earlier findings); GPS presence only escalates 0→1; `metadata_source`
escalates to `'exif'` once real values have landed. A readable image with no
EXIF segment is a *success* (empty record, still stamped), not a failure; a
file that cannot be parsed at all is a friendly per-file error. Orientation
is derived from the best-known (scanner ∪ EXIF) width×height by the pass.

**Date estimation (Sprint 12):** photos without an EXIF date get a
`capture_datetime` estimate with `capture_datetime_source` set to the
provenance — `'filename'` when the name parses as a date, `'mtime'`
otherwise. See IMAGE_ANALYSIS.md ("Date estimation") for the full rules;
exactly one resolution applies per photo, in dominance order
(exif > filename > mtime), and an estimate is only written when the photo
had no date at all (`COALESCE` — it never overrides a real EXIF date).

### analysis
One row per analyzed photo (`photo_id` PK, FK cascade).

sharpness, brightness, contrast, saturation (0–100), highlight_clipping,
shadow_clipping (percent), is_monochrome/is_dark/is_bright (0/1),
face_count, smile_count (nullable until local AI runs; face_count is
written by the Sprint 9 face pass, smile_count is v0.2 and stays NULL),
perceptual_hash (hex, Sprint 8), algorithm_version (INTEGER, see below),
analyzed_at, source_mtime (v6: RFC3339 mtime of the file the row was
computed from; NULL on pre-v6 rows), faces_at (v10: the file mtime the
face pass stamped `face_count` from; NULL until it runs).

**Versioning + incremental rule:** `algorithm_version` records which math
produced the row (`ANALYSIS_ALGORITHM_VERSION`, currently 1); bumping it
makes every stale row re-analyzable. On top of that, `source_mtime` (v6)
gives per-file incrementality: `analysis_queue` selects a photo iff it has
no row, `algorithm_version` is older than the current constant, or
`source_mtime IS NOT file_mtime` (the `IS NOT` is NULL-safe: a pre-v6 row
and a never-stat'd file compare "equal"). A re-scan refreshes
`photos.file_mtime`, so a file that changed on disk is picked up by the
next analysis pass automatically. `upsert_analysis` updates only the columns
the analysis pass owns — it never clobbers perceptual_hash (Sprint 8) or
face/smile columns (Sprint 9) when it re-measures brightness etc. Scores
are normalized 0–100 so filters/UI stay stable across versions.

**Face-only rows (Sprint 9):** the face pass can store a count for a photo
the analysis pass hasn't measured yet. `upsert_faces` then inserts a row
with `face_count`/`faces_at` set and every measurement `NULL` (its
`analyzed_at`/`algorithm_version` are filled so the NOT NULL constraints
hold). Such a row is exactly like a row awaiting analysis: `source_mtime`
is `NULL`, so the NULL-safe `source_mtime IS NOT file_mtime` comparison
keeps it in `analysis_queue()` until the analysis pass fills the
measurements — and that pass's upsert preserves the face columns (integrated-
tested in `tests/ml_integration.rs`).

### collections / collection_photos
Manually curated sets (Sprint 8 UI). `collection_photos` is the join table
with composite PK. Deleting a collection removes only its membership rows
(FK cascade) — never the photographs. A collection may contain a photo that
is not in any session (NULL `session_id`), and membership is orthogonal to
culling: adding to a collection never touches `selections` or files.

### saved_views
Dynamic filters: `filter_json` holds the full structured filter
(see FILTER_ENGINE.md), so a view stays correct as the library changes.
Uniqueness on `name` (saving with an existing name overwrites that view's
filter + description, keeping the same `id` — `updated_at` moves). The UI's
live "photograph count" is **not** stored: it is recomputed on demand by
feeding `filter_json` through the same filter engine the grid uses.

### similarity_groups / similarity_group_photos
Groups found by the similarity pass (Sprint 8, see SIMILARITY.md).
`group_type` ∈ `similar` (perceptual-hash cluster within one session) |
`burst` (photographs captured within `BURST_WINDOW_SECS` of each other);
`hash` labels the group (hex dHash for similar groups, `burst:<epoch secs>`
for bursts) and `photo_count` is denormalized. The whole group set is
**replaced atomically** on each pass (`replace_similarity_groups` in one
transaction), so a group set always reflects the current hashes — partial
state is impossible. `similarity_group_photos` is the join with composite
PK; up to the first 4 member ids (by id order) are surfaced as `cover_photos`
by the list query for UI cover strips.

### file_operations
Audit log for every rename/move/copy/trash: op_type, source, destination,
status, detail, timestamp. This underpins "what happened to my files?" and
the selection-ratio statistics.

### selections
Culling state (v8, Sprint 6 infrastructure; written by the Sprint 7
selection UI): one row per photo.

| column | type | notes |
|---|---|---|
| photo_id | INTEGER PK → photos | ON DELETE CASCADE |
| state | TEXT | `selected` \| `rejected` (CHECK constraint) |
| updated_at | TEXT | RFC3339 |

The statistics engine treats it as *a* selection signal; an empty table (plus
no move/copy/rename/trash operations) means "no selection signal" and the
ratio section is hidden entirely, not zeroed.

### app_settings
Key/value for application state. Keys in use: `active_folder` (the scanned
folder — persisted on open, restored on next start, and auto-cleared by
`get_active_folder` when the path no longer exists on disk, so a deleted or
renamed folder is never resurrected), `ai_enabled` (Sprint 9: local-intelligence
preference, `"true"` / `"false"`, **off by default** — turning it on gates the
post-scan face pass auto-run, it never forces inference).

### schema_version
`version`, `applied_at`.

  ## Query surface (as of Sprint 9) 

- `upsert_photo` / `upsert_session` / `refresh_session_counts` /
  `refresh_all_sessions_times` / `list_sessions` — scanner ingest (Sprint 2).
  `refresh_session_counts(id)` re-derives one session's `photo_count` and
  its `start_time`/`end_time` from
  `COALESCE(capture_datetime, indexed_at)` (empty sessions → NULL times);
  `refresh_all_sessions_times()` does the same for every session in one
  statement and runs after each scan pass and after the metadata pass (the
  pass is what fills capture datetimes).
- `photos_where(where_sql, where_params, offset, limit)` — the single
  paginated grid query (limit clamped to 1–500). Takes a pre-built,
  **parameterized** `WHERE` clause (produced by the filter engine; see
  FILTER_ENGINE.md) plus its bound parameters in order; `LIMIT ?`/`OFFSET ?`
  are appended. `LEFT JOIN analysis` gives filters (and `has_analysis`)
  access to analysis columns under alias `a`. Ordered by capture date
  (unknowns first, then id) so the grid is stable. Full analysis arrives per
  photo via `get_photo_full`.
- `list_photos(offset, limit)` — thin unfiltered alias of `photos_where`
  (`WHERE ""`), the default grid path.
- `get_photo_full(id)` — one photo with its EXIF + `analysis` row via
  `LEFT JOIN` (analysis fields are `NULL`/`false` until the analysis pass
  runs), used by the viewer's metadata panel.
- `analysis_queue(extensions)` — photos still needing analysis (left join,
  NULL-safe mtime comparison, capture-time ordering, see rule above).
- `upsert_analysis(photo_id, metrics, source_mtime)` — idempotent by
  `photo_id`; updates only analysis-owned columns.
- `analysis_progress_counts(extensions)` — (decodable photos, analyzed of
  them) for the status line.
- `exif_queue()` — photos the metadata pass hasn't read yet
  (`exif_at IS NULL`), in capture-time order, carrying current dimensions.
- `upsert_exif(photo_id, record)` — merge one file's EXIF extraction
  (`COALESCE`, GPS 0→1 only, stamps `exif_at`); see the photos section.
- `status()` — returns `metadata_pending` (count of `exif_at IS NULL`),
  `selected_count` / `rejected_count` (Sprint 7 culling totals),
  `faces_done` (Sprint 9: count of `face_count IS NOT NULL`), alongside the
  existing counts.

### File operations (Sprint 7)

- `update_photo_path(id, path, filename, size_bytes, file_mtime)` — re-point a
  photo after a successful rename/move (pixels unchanged, so the analysis row
  stays valid; `file_mtime` is refreshed for the incremental rule).
- `delete_photos(ids)` — remove rows after a successful trash; FK cascade
  removes their analysis + selection rows.
- `record_file_op(op_type, source_path, dest_path, status, detail)` — append
  one row to the `file_operations` audit log (the v4 table, written from Sprint
  7 on). Every executed/attempted item is recorded with `done`/`failed` + a
  reason.
- `recent_file_ops(limit)` — audit log, newest first (limit clamped 1–500).
- `set_selection(photo_id, state)` / `set_selections(ids, state)` /
  `clear_selection(id)` / `clear_selections(ids)` — culling state on the v8
  `selections` table; `state` is a closed set (`selected`/`rejected`), anything
  else is a friendly validation error.
- `list_selections(limit)` — the current culling map (id → state), capped at
  20,000.

### Saved views (Sprint 8)

- `list_saved_views()` — all views, alphabetical.
- `upsert_saved_view(name, filter_json, description) -> id` — create or
  overwrite by name (names trimmed; blank → validation error; `UNIQUE(name)`
  violation → "already exists" style friendly error). The view's `filter_json`
  is validated with the grid's own engine by the command layer before this is
  called, so a stored view is always evaluable.
- `rename_saved_view(id, name)` / `delete_saved_view(id)`.
- (The dynamic count is `photos_where` over the view's filter — see
  FILTER_ENGINE.md — not a separate surface.)

### Collections (Sprint 8)

- `list_collections()` — with live `photo_count` (subquery per row).
- `create_collection(name, description) -> id` (names trimmed; uniqueness →
  friendly error), `rename_collection(id, name)`, `delete_collection(id)`
  (membership cascades, photos untouched).
- `add_to_collection(collection_id, photo_ids) -> added` — idempotent
  (`INSERT OR IGNORE`), returns how many rows were actually new; the
  collection must exist (friendly error otherwise).
- `remove_from_collection(collection_id, photo_ids) -> removed`.
- `collection_photos(collection_id, offset, limit)` — the collection's
  photographs as `PhotoSummary` pages (join through `collection_photos`
  ordered by `added_at`), the same row shape the library grid uses.
- `collection_ids_for_photos(photo_ids)` — which collections contain each
  given photo (membership badges; dynamic `IN ?` placeholders).

### Similarity (Sprint 8)

- `phash_queue()` — photos that need (re-)hashing: `phash IS NULL` or
  (`phash_source_mtime` recorded AND `file_mtime` known AND newer than the
  recorded one), restricted to decodable extensions (the scanner's
  `DECODABLE_EXT` list), in capture-time order. The mirror of `analysis_queue`
  for the hash pass.
- `upsert_phash(photo_id, hash, source_mtime)` — persist one 64-bit hash
  (stored as `INTEGER`) + the mtime it was computed from.
- `hashed_photos()` — all rows with `phash IS NOT NULL`, returning
  `(id, hash, session_id, capture_datetime)` — the similarity pass's
  grouping input (session + capture time are how groups stay scoped to a
  shoot, see SIMILARITY.md).
- `list_similarity_groups(limit)` — current groups, bursts first then by
  size (stable by `id`), each with ≤4 `cover_photos`.
- `group_photos(group_id, offset, limit)` — a group's photographs as
  `PhotoSummary` pages ordered by capture time (an empty/unknown group is a
  clean `( [], 0 )`, not an error).
- `replace_similarity_groups([(hash, group_type, photo_ids)]) -> count` — one
  transaction: delete all groups + memberships, insert the full new set
  (atomic replacement; the pass is the only writer).

### Local intelligence (Sprint 9)

- `faces_queue()` — photos the face pass must (re-)detect: `face_count IS
  NULL` or `file_mtime` newer than the `faces_at` stamp, decodable
  extensions only, capture-time order. The mirror of `phash_queue` for the
  face pass (see LOCAL_AI.md).
- `upsert_faces(photo_id, face_count, source_mtime)` — store one photo's
  result, idempotent by `photo_id`; creates a face-only analysis row where
  needed (see the analysis section) and the update path touches only the
  face columns — never the measurements, never `source_mtime`.
- `faces_done()` — count of photos with a stored result (Settings line).

## Conventions

- All timestamps UTC RFC3339 (see `time.rs`).
- Counts that drive UI (session photo_count) are denormalized and updated
  transactionally on ingest — statistics engine still computes from the base
  tables for anything non-trivial.
- Never store GPS coordinates, never store raw pixel data.
