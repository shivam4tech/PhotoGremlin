# Database

SQLite, single file: `<data_dir>/database.sqlite` (see DEVELOPMENT.md for
per-OS locations). WAL mode, `PRAGMA foreign_keys=ON`, one
`Mutex<Connection>` in `src-tauri/src/database.rs`.

## Migration policy

Version stored in `schema_version (version, applied_at)`. Migrations are
idempotent batches applied at startup up to `CURRENT_SCHEMA_VERSION`
(currently 8). Tests assert both expected-table presence and idempotency.

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
  `selected` | `rejected`) the statistics engine reads for the selection
  ratio (the selection UI lands in Sprint 7)

## Tables (schema v1–v8)

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
| focal_length | REAL | mm (1/100 mm EXIF tag converted to mm) |
| iso | INTEGER | |
| aperture | REAL | f-number, e.g. 2.8 |
| shutter_speed | REAL | seconds (e.g. 1/250 = 0.004) |
| capture_datetime | TEXT | UTC RFC3339, from EXIF. EXIF stores a zone-less local clock time; PhotoGremlin stores that wall-clock time **verbatim as UTC** (a documented decision — the catalog stays lexicographically sortable for filters without silently assuming the user's timezone) |
| gps_present | INTEGER | 0/1 — presence only. **Coordinates are never stored** (see PRIVACY.md) |
| session_id | INTEGER → sessions | ON DELETE SET NULL |
| indexed_at | TEXT | |
| file_mtime | TEXT | file mtime; incremental analysis reuses rows when mtime+size unchanged |
| exif_at | TEXT | (v7) RFC3339 time the metadata pass read this file; NULL = not yet read. One read per file in v0.1 |

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

**Metadata (EXIF) merge (Sprint 5):** the metadata pass (`exif_queue` →
`upsert_exif`) reads each file once and stamps `exif_at`, so re-runs are
no-ops and `status().metadata_pending` (count of `exif_at IS NULL`) drives
the UI. `upsert_exif` merges with `COALESCE`: dimensions/orientation already
resolved by the scanner win, GPS presence only escalates 0→1, and the other
EXIF columns are filled from the extraction. A readable image with no EXIF
segment is a *success* (empty record, still stamped), not a failure; a file
that cannot be parsed at all is a friendly per-file error. Orientation is
derived from the best-known (scanner ∪ EXIF) width×h by the pass.

### analysis
One row per analyzed photo (`photo_id` PK, FK cascade).

sharpness, brightness, contrast, saturation (0–100), highlight_clipping,
shadow_clipping (percent), is_monochrome/is_dark/is_bright (0/1),
face_count, smile_count (nullable until local AI runs, Sprint 9),
perceptual_hash (hex, Sprint 8), algorithm_version (INTEGER, see below),
analyzed_at, source_mtime (v6: RFC3339 mtime of the file the row was
computed from; NULL on pre-v6 rows).

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

### collections / collection_photos
Manually curated sets (Sprint 8 UI). `collection_photos` is the join table
with composite PK.

### saved_views
Dynamic filters: `filter_json` holds the full structured filter
(see FILTER_ENGINE.md), so a view stays correct as the library changes.
Composite uniqueness on `name`.

### similarity_groups / similarity_group_photos
Groups of visually similar photos (perceptual-hash clusters) and
burst-like time clusters (`group_type` distinguishes).

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
Key/value for application state (e.g. `active_folder`).

### schema_version
`version`, `applied_at`.

## Query surface (as of Sprint 6)

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
- `status()` — returns `metadata_pending` (count of `exif_at IS NULL`)
  alongside the existing counts.

## Conventions

- All timestamps UTC RFC3339 (see `time.rs`).
- Counts that drive UI (session photo_count) are denormalized and updated
  transactionally on ingest — statistics engine still computes from the base
  tables for anything non-trivial.
- Never store GPS coordinates, never store raw pixel data.
