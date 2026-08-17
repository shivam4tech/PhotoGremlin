# Database

SQLite, single file: `<data_dir>/database.sqlite` (see DEVELOPMENT.md for
per-OS locations). WAL mode, `PRAGMA foreign_keys=ON`, one
`Mutex<Connection>` in `src-tauri/src/database.rs`.

## Migration policy

Version stored in `schema_version (version, applied_at)`. Migrations are
idempotent batches applied at startup up to `CURRENT_SCHEMA_VERSION`
(currently 5). Tests assert both expected-table presence and idempotency.

- v1: core tables (sessions, photos, analysis, app_settings)
- v2: collections
- v3: saved_views + similarity groups
- v4: file_operations audit log
- v5: partial unique index `sessions(root_path) WHERE root_path IS NOT NULL`
  — one session per imported folder (manual sessions keep `root_path` NULL)
- v6: `analysis.source_mtime TEXT` (NULL on pre-v6 rows) — the source file
  mtime each analysis row was computed from (idempotency check, see below);
  added via `ALTER TABLE`, guarded by a `PRAGMA table_info` probe

## Tables (schema v1–v6)

### sessions
A shoot or imported body of work.

| column | type | notes |
|---|---|---|
| id | INTEGER PK | |
| name | TEXT | human name (e.g. folder name) |
| root_path | TEXT | the scanned folder, if any |
| start_time / end_time | TEXT | UTC RFC3339, from photo capture datetimes |
| photo_count | INTEGER | denormalized counter, maintained on ingest |
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
| camera_make / camera_model / lens | TEXT | EXIF (Sprint 5 fills these) |
| focal_length | REAL | mm |
| iso | INTEGER | |
| aperture | REAL | e.g. 2.8 |
| shutter_speed | REAL | seconds |
| capture_datetime | TEXT | UTC RFC3339, from EXIF |
| gps_present | INTEGER | 0/1 — presence only. **Coordinates are never stored** (see PRIVACY.md) |
| session_id | INTEGER → sessions | ON DELETE SET NULL |
| indexed_at | TEXT | |
| file_mtime | TEXT | file mtime; incremental analysis reuses rows when mtime+size unchanged |

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

### app_settings
Key/value for application state (e.g. `active_folder`).

### schema_version
`version`, `applied_at`.

## Query surface (as of Sprint 4)

- `upsert_photo` / `upsert_session` / `refresh_session_counts` /
  `list_sessions` — scanner ingest (Sprint 2).
- `list_photos(offset, limit)` — paginated grid rows (limit clamped to 1–500).
  Ordered by capture date (unknowns first, then id) so the grid is stable.
  `LEFT JOIN analysis` only to compute a `has_analysis` flag — the grid
  payload stays small; full analysis arrives per photo via `get_photo_full`.
- `get_photo_full(id)` — one photo with its `analysis` row via `LEFT JOIN`
  (analysis fields are `NULL`/`false` until the analysis pass runs), used by
  the viewer's metadata panel.
- `analysis_queue(extensions)` — photos still needing analysis (left join,
  NULL-safe mtime comparison, capture-time ordering, see rule above).
- `upsert_analysis(photo_id, metrics, source_mtime)` — idempotent by
  `photo_id`; updates only analysis-owned columns.
- `analysis_progress_counts(extensions)` — (decodable photos, analyzed of
  them) for the status line.

## Conventions

- All timestamps UTC RFC3339 (see `time.rs`).
- Counts that drive UI (session photo_count) are denormalized and updated
  transactionally on ingest — statistics engine still computes from the base
  tables for anything non-trivial.
- Never store GPS coordinates, never store raw pixel data.
