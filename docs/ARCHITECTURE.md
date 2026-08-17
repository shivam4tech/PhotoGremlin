# Architecture

One shared codebase, three layers. The frontend never touches files, pixels, or
the database directly.

```
React UI (React 18 + TypeScript + Zustand)
   │  typed IPC — src/lib/ipc.ts is the single funnel for invoke()/listen()
   ▼
Tauri commands (src-tauri/src/commands/*)   ← thin, validated entry points
   │
     ├── Domain services
     │     scanner/     recursive indexing
     │     thumbnailer  cached thumbnail engine (grid/viewer JPEG, base64 out)
     │     analysis/    image measurement pipeline (background tasks)
     │     metadata/    EXIF extraction + metadata pass (background tasks)
     │     filters/     structured filter engine (pure: JSON → parameterized WHERE)
     │     similarity/  perceptual hash + grouping
    │     statistics/  period-scoped aggregation (UI-independent)
    │     filesystem/  rename/move/copy/trash safety rules
    │     ml/          optional local models (isolated; AI-optional)
    │
   ├── database.rs    SQLite via rusqlite (bundled), Mutex<Connection>
   │
   └── paths.rs       OS data/cache/log locations (Tauri path resolver)
```

## Module notes (what actually exists today)

- `lib.rs` — app entry. Wires plugin (dialog), creates `AppState { db, paths }`,
  registers commands. No business logic.
- `state.rs` — `AppState` (Arc of `Db` + `AppPaths` + `ThumbService` + the
   scan-job slot + the analysis-job slot + the metadata-job slot + the
   file-operation slot + the similarity slot + the faces slot), shared via
   Tauri's managed state. Commands take `State<AppState>`. Each slot holds
   the live `Job { running, cancel }` Arcs: a claim-and-cancel mechanism
   shared between `start_*`/`stop_*` and the background task (scan, analysis,
   metadata, file operations, similarity and face detection are separate
   slots; the UI keeps them mutually exclusive).
- `thumbnailer.rs` — the local thumbnail engine (Sprint 3). One `ThumbService`
  per app holds: the cache dir, a generation semaphore
  (`THUMB_GENERATE_CONCURRENCY = 3` full-res decodes at most), and an
  in-flight dedup map (waiters poll the cache file with a 10 s bounded
  deadline instead of decoding twice). `get(db, photo_id, kind)` = photo row
  lookup → previewable-format check (RAW/HEIC return a friendly unsupported
  error the UI renders as a labelled placeholder) → missing-file check →
  cache hit? → else `spawn_blocking` generation: header-only
  `image_dimensions` check (≤ ~500 MP guard) → `resize_exact` (triangle
  filter) → JPEG q82 → atomic temp+rename cache write. The UI receives base64
  data URLs (grid ≤ 256 px wide, viewer ≤ 1600 px) — full-resolution files
  never enter the webview. Cache key = 16-hex FNV-1a of
  `path|size|mtime|width|THUMB_VERSION` (std's `DefaultHasher` is randomly
  seeded and must not be used for cache keys); version bump invalidates all.
  Base64 is a ~40-line local impl (round-trip tested), not a dependency.
- `commands/photos.rs` — `list_photos` (paginated grid), `get_photo_full`
  (viewer metadata), `get_thumbnail` (async; clones the state Arcs and drops
  the `State` guard before awaiting — Tauri command futures must be `Send`).
- `analysis/` (Sprint 4) — local image analysis. `metrics.rs` holds pure,
  Tauri/I-O-free math on an `RgbImage` (Rec.709 luma → one 256-bin histogram
  pass yields brightness/contrast/clipping/percentiles; per-pixel saturation;
  2× monochrome gates; 4-neighbor Laplacian variance → sigmoid-scaled
  sharpness). All constants are pinned and documented with calibration notes
  (see IMAGE_ANALYSIS.md). `mod.rs` runs the pass: `analysis_queue` →
  round-robin slices to `ANALYSIS_WORKERS` std threads → bounded decode at
  `WORKING_MAX_SIDE` → measure → `upsert_analysis` → per-item progress.
  Cancellation is cooperative and checked per item.
- `commands/analysis.rs` — `start_analysis` (claims the analysis slot,
  `spawn_blocking`s the pass, returns immediately) / `stop_analysis`;
  `analysis-progress` + `analysis-complete` events carry progress and the
  `AnalysisSummary { analyzed, failed, cancelled, elapsed_ms, errors }`.
- `metadata/` (Sprint 5) — local EXIF extraction + the metadata pass.
  `exif.rs` is pure (path → `ExifRecord`): opens the file once, reads EXIF
  with the `kamadak-exif` reader, and maps tags to camera make/model/lens,
  focal length (1/100 mm → mm), ISO, f-number, exposure seconds, capture
  datetime (zone-less EXIF clock stored verbatim as UTC RFC3339), and a
  **presence-only** GPS bit — coordinates never reach the record struct. A
  readable image with no EXIF is an empty record (not an error); an
  unparseable file is a friendly error. `mod.rs` runs the pass, mirroring the
  analysis pipeline: `exif_queue` → round-robin slices to
  `METADATA_WORKERS = 3` std threads → `upsert_exif` (COALESCE merge, stamps
  `exif_at`) → per-item progress; cooperative cancel; a 256 MB per-file guard.
  Re-runs are no-ops (queue drains to `exif_at IS NULL` only).
- `commands/metadata.rs` — `start_metadata` (claims the metadata slot,
  `spawn_blocking`s the pass, returns immediately) / `stop_metadata`;
  `metadata-progress` + `metadata-complete` events carry the
  `MetadataSummary { processed, failed, cancelled, elapsed_ms, errors }`.
  The auto-run: the UI fires `start_metadata` as soon as `scan-complete` has
  indexed new photos (the pass is a cheap no-op when nothing is new).
- `filters/` (Sprint 5) — the structured filter engine, pure and
  Tauri/DB-independent (see FILTER_ENGINE.md). Parses + validates the filter
  JSON (a fixed **field registry** of compile-time SQL expressions maps each
  field to (expression, kind); unknown fields / operators / value types fail
  with a friendly `Validation` error before any SQL), then lowers it to a
  parameterized `WHERE` fragment + an ordered `SqlParam` vector. Column names
  come only from the registry; **every user value is a bound parameter**
  (injection-safe). `commands/filters.rs::list_filtered_photos` = parse →
  build → `Db::photos_where`. The grid, saved views, and statistics all share
   this one object.
- `statistics/` (Sprint 6) — the statistics engine, a UI-independent service
  (see STATISTICS.md). One `Period` model (today / this-week (Monday-based) /
  this-month / this-year / custom / all) resolves against an injected `now`
  to an RFC3339 string range; query time source is
  `COALESCE(p.capture_datetime, p.indexed_at)` — string comparison IS time
  comparison because the catalog stores UTC RFC3339. `stats_for_scope` runs a
  fixed set of aggregate queries for a scope (period OR one session): totals,
  analyzed-only averages, mono/color shares, face/smile shares (only over
  photos with AI data), the four EXIF histograms (column values fetched,
  binned by the pure `bins.rs` functions — fixed bins, documented edges),
  camera/lens usage (top 20 by count, share + analyzed-only avg
  sharpness/ISO, NULL names → "Unknown camera/lens"), the monthly trend
  (only months with data, newest 36, chronological out), and the selection
  ratio (present only when a selection signal exists: `selections` state in
  scope, or move/copy/rename/trash rows in `file_operations`). **Honest data
  is a type-level rule**: every average/share is `Option` and stays `None`
  when its inputs do not exist; the UI renders that as "unavailable", never
  0. `session_summary` = the same core scoped to one session + duration;
  `compare_sessions` = up to 8 sessions on the same metric rows.
 - `commands/stats.rs` — `period_stats` (arg: `periodJson`),
   `session_summary(sessionId)`, `compare_sessions(sessionIds)`. Synchronous
   pure-SQL commands; no background task, no events.
- `similarity/` (Sprint 8) — perceptual hashing + grouping, Tauri-free and
   integration-tested (see SIMILARITY.md). `dhash64` (9×8 difference hash),
   `hamming`, `photo_hash` (decode → grayscale → hash), `group_similar`
   (union-find, threshold `SIMILAR_THRESHOLD = 8`, components ≥ 2),
   `group_bursts` (capture times within `BURST_WINDOW_SECS = 3`, ≥ 2, no
   time → never joins). `run_similarity(db, progress, cancel)` = the pass:
   hash `phash_queue()` (incremental on `phash_source_mtime`) → group within
   each session → `replace_similarity_groups` (atomic swap). Cancellation is
   per-file; grouping still completes so the app ends on a consistent set.
- `commands/similarity.rs` — `start_similarity` (claims the similarity slot,
   `spawn_blocking`s the pass) / `stop_similarity`; `similarity-progress` +
   `similarity-complete` events carry the `SimilaritySummary { hashed, failed,
   similar_groups, burst_groups, elapsed_ms, cancelled }`.
- `ml/` (Sprint 9) — local intelligence: face detection behind the
  isolation boundary (see LOCAL_AI.md). The model (YuNet 2023mar, 232 KB,
  Apache-2.0) is `include_bytes!`-ed into the binary; the ONNX Runtime is
  dlopened from the system via ort in `load-dynamic` mode (pinned
  `ort = 2.0.0-rc.9` / `ort-sys = 2.0.0-rc.9` — the details of why are in
  LOCAL_AI.md). `runtime_status()` probes the library with `libloading`
  **before** any ort call (ort panics instead of erroring on a missing
  library), so a machine without the runtime gets a friendly status + a
  refusing pass, never a crash. Pure, unit-tested halves: `build_blob`
  (640² distorted linear resize, BGR, per-channel mean 104/177/123, CHW
  f32) and `decode_detections` (offset-0 anchors, stride-scaled deltas,
  `exp` sizes, `sqrt(cls·obj)` score, NMS IoU 0.3, top-100) plus `iou`/`nms`.
  `run_faces_pass(db, progress, cancel)` = the pass: `faces_queue()` →
  per-file guards (256 MB / 250 MP) → decode → one shared `ort::Session`
  (`Send + Sync`, built once per pass) → `upsert_faces` + stamp. Sequential
  by design (independent per-file work; the queue is small because it is
  incremental); cancellation between files.
- `commands/ai.rs` — `ai_status` (enabled, runtime availability + friendly
  note, model provenance/size, `faces_done`/`photo_count`), `set_ai_enabled`
  (persists the preference; does not start anything), `start_faces`
  (claims the faces slot, `spawn_blocking`s the pass) / `stop_faces`;
  `faces-progress` + `faces-complete` events carry the `FaceSummary
  { processed, with_faces, failed, cancelled, elapsed_ms, errors }`.
  Auto-run: the UI starts the pass after a scan that indexed new photos,
  only when the stored `ai_enabled` preference is on.
- `commands/views.rs` + `commands/collections.rs` (Sprint 8) — saved views and
    collections over the v2/v3 tables. Views: `list_saved_views` /
   `save_view` (validated with the grid's own filter engine before persist) /
   `rename_saved_view` / `delete_saved_view` / `saved_view_count` (dynamic —
   recomputes `photos_where` over the stored filter). Collections: `list_` /
   `create_` / `rename_` / `delete_collection` (deletes membership, never
   photos) + `add_to_collection` (idempotent, returns count added) /
   `remove_from_collection` / `collection_photos` (paged `PhotoSummary`, the
   same shape the grid uses). All synchronous; files are never touched.
- `filesystem/` (Sprint 7) — rename/move/copy/trash behind the universal
  safety protocol (see FILE_OPERATIONS.md). Tauri-independent
  (`plan_rename` / `plan_move_copy` / `plan_trash` + `run_operation(db, plan,
  progress, cancel)`), integration-tested on real temp dirs. Pure template
  engine (`expand_template` single-pass, `sanitize_name`, `{sequence}`
  zero-pad), fixed-bins-free. Rename = atomic in-dir `rename`; in-plan name
  collisions abort the whole plan, on-disk collisions block the item. Move/copy
  = `fs::rename` with `CrossesDevices` → staged copy→size-verify→delete; copy
  never touches the original and indexes the copy. Trash = freedesktop XDG
  trash (Linux) with `.trashinfo`; never permanent delete. Every executed item
  updates the photo row (rename/move) or removes it (trash) and appends a
  `file_operations` audit row. Execution re-checks each destination right
  before acting so preview→confirm races are per-item failures, not
  overwrites.
- `database.rs` — single `Mutex<Connection>`; short critical sections only
  (never held across await). Versioned schema in `schema_version`
  (see DATABASE.md). WAL mode, foreign keys on.
- `scanner/` — recursive, two-pass folder scan (enumerate → index).
  Tauri-free core (`run_scan(db, progress, cancel)`) so the exact pipeline
  that ships is unit/integration-tested in `tests/scan_integration.rs` with
  synthetic images. Classifies decodable (jpg/png/webp/tiff), RAW (indexed,
  no pixels yet) and HEIC (indexed, placeholder preview) vs ignored; skips
  hidden dot-directories; cancels cooperatively between files.
- `error.rs` — one `AppError` enum. Its `Display` text is what the UI shows
  (friendly); `tracing::error!` inside constructors keeps the details in the
  local log. `From<AppError> for tauri::ipc::InvokeError` makes `Result<T,
  AppError>` a valid command return type.
- `events.rs` — IPC event names (`scan-progress`, `scan-complete`,
  `analysis-progress`, `analysis-complete`, `metadata-progress`,
  `metadata-complete`, `operation-progress`, `operation-complete`,
  `similarity-progress`, `similarity-complete`, `faces-progress`,
  `faces-complete`, `db-changed`) +
  `ProgressPayload { total, done, stage, current }` (stages: discovering,
  indexing, analyzing, reading metadata, hashing, grouping, detecting faces,
  done, and the operation verb rename/move/copy/trash). Progress flows
  Rust → UI via Tauri events, never by polling. Every `*-complete` event
  carries `{ summary?, error? }` (`operation-complete`'s summary is
  `OperationSummary`: per-item done/failed/skipped/cancelled, capped at 500
  for IPC; `similarity-complete`'s is `SimilaritySummary`, above;
  `faces-complete`'s is `FaceSummary` above).
- `logging.rs` — `tracing` with a rolling daily file in the Tauri log dir
  (`<data_dir>/logs/photogremlin.<date>.log` on Linux) + console layer.
  Zero telemetry; the only sink is the local disk.
- `time.rs` — all stored timestamps are UTC RFC3339.

## Concurrency model

- Tauri runs commands on its thread pool. Pattern in use (scans Sprint 2,
  analysis Sprint 4): the command claims a single job slot in `AppState`,
  spawns the CPU/IO work via `tauri::async_runtime::spawn_blocking`, and
  returns immediately. Completion + summary are pushed as IPC events
  (`scan-progress`/`scan-complete`, `analysis-progress`/`analysis-complete`).
  The analysis pass runs `ANALYSIS_WORKERS = 3` std threads decoding at a
  bounded working resolution (at most a handful of images in memory at
  once); file ops (Sprint 7) follow the same shape.
- The UI always stays responsive: nothing blocks on the webview thread, and
  progress events keep the spinner honest.
- SQLite: one writer at a time (the Mutex). Writes are incremental per photo,
  so results appear while the pipeline runs.

## Frontend rules

- Views (`src/views/*`) render; features (`src/features/*`) hold logic;
  `stores/appStore.ts` (Zustand) holds app state.
- All backend calls go through `src/lib/ipc.ts` (typed, typed errors via
  `toErrorMessage`). No raw `invoke` anywhere in views.
- `src/types/api.ts` mirrors Rust serde types 1:1 — when a Rust type changes,
  the TS mirror changes in the same commit.
- The library grid is virtualized (`components/VirtualGrid.tsx`, a
  dependency-free vertical windower) and paginated (96 tiles/page via
  `hooks/useFilteredPhotos.ts`, which always routes through the structured
  filter — the unfiltered grid is the empty filter); each tile requests
  exactly one grid-size thumbnail (`components/PhotoTile.tsx`). The UI must
  never request full-resolution images for the grid. The viewer
  (`features/viewer/Viewer.tsx`) loads a viewer-size thumbnail +
  `get_photo_full` for the metadata panel; ←/→ move within the loaded page,
  Esc closes.
 - Filtering (Sprint 5, hoisted to the store in Sprint 8): the active filter is
   **structured data** held in `stores/appStore.ts` (`filterConditions:
   FilterCondition[]`) — in the store (not a view) so the Sessions view
   ("Open in library") and saved views ("open") can set it from elsewhere and
   navigate to the Library — and rendered by `features/library/FilterBar.tsx`.
   The pure registry + helpers live in `features/library/filterFields.ts`
   (field/operator knowledge, chip labels, condition composition) and are
   unit-tested in `src/tests/filterFields.test.ts`. Changing the filter
   re-loads page 0; the exact `Filter` object is stringified and sent to the
   engine (and is what a saved view stores).
 - Saved views + collections (Sprint 8): `views/SavedViewsView.tsx` lists
   views with their **live** counts (recomputed, never stored), applies one by
   loading its `filter_json` into `filterConditions` + navigating, and
   renames/deletes. "Save as view" in the Library toolbar captures the current
   `filterConditions`. `views/CollectionsView.tsx` creates/opens/deletes
   collections and renders an opened collection in the same grid + viewer
   (`collection_photos` paged); membership is edited from the Library's
   culling bar ("Add to collection"). The pure naming/labeling rules
   (`cleanName`, `groupLabel`) live in `features/organize/labels.ts`
   (unit-tested, `src/tests/organizeLabels.test.ts`).
  - Similarity (Sprint 8, frontend): the Library's "Find similar photos"
    toolbar button starts the pass (progress via `similarity-progress`); on
    completion the "Similar groups" panel shows group cards (cover strips via
    `features/similarity/CoverThumb.tsx`, factual labels via `groupLabel`).
    Clicking a card opens that group's photographs in the same grid + viewer
    path (`group_photos`), with a back bar. The language is kept factual
    ("near-identical structure", "captured within seconds") — the user decides
    via culling + file ops.
  - Local intelligence (Sprint 9, frontend): configured only from the
    Settings "Local intelligence" card (`views/SettingsView.tsx` + pure
    wording helpers in `features/settings/ai.ts`, unit-tested in
    `src/tests/settingsAi.test.ts`): on/off toggle (off by default), runtime
    availability line (with the friendly note when ONNX Runtime is missing),
    model provenance + embedded size, "N of M checked for faces", run-now /
    stop buttons with live progress, last-pass summary. `App.tsx` owns the
    `faces-progress` / `faces-complete` listeners and the post-scan auto-run
    (only when the stored preference is on). The dashboard's face statistics
    (`faces_present_share`) were already wired in Sprint 6 against the
    `face_count` column and now get their data from this pass.
- Statistics (Sprint 6): `views/DashboardView.tsx` renders
  `PeriodStats` for the selected period (+ custom range) — totals,
  analyzed-only averages, shares, the four distributions (pure CSS bars),
  camera/lens usage tables, monthly trend, and the selection section (when a
  signal exists). `views/SessionsView.tsx` adds 2–8 session comparison
  (`compare_sessions`) and a per-session detail (`session_summary`). Pure
  formatting + the honest-"unavailable" rendering live in
   `features/stats/format.ts` (unit-tested in `src/tests/statsFormat.test.ts`);
   the language discipline ("sharpness 62", never "you improved") is enforced
   there and in the view copy.
 - File operations (Sprint 7): culling (keep/reject) renders as per-tile
   controls in `components/PhotoTile.tsx`, driven by `stores/appStore.ts`
   (`selections` map + `selectionMode`, persisted to the `selections` table via
   `set_selections`/`clear_selections`). With photographs kept,
   `features/fileops/FileOpsPanel.tsx` offers rename / move / copy / trash —
   every action builds a backend **plan** (`plan_*`) the UI previews (per-item
   mapping, blocked items, `will_create_dir`, red aborted state) and confirms
   (trash uses a native `ask` warning) before `start_*` runs it in the
   background. `operation-progress`/`operation-complete` stream the run; the
   panel shows a live progress line, a Stop button, and a results summary of
   anything not `done`. Pure wording helpers (factual verbs, preview/result
   headlines, progress label) live in `features/fileops/format.ts` (unit-tested
   in `src/tests/fileopsFormat.test.ts`). After any operation the app
   refetches status, culling, the audit log, and the grid (`libraryVersion`).

## Error flow

1. Rust logs detail locally (`tracing::error!`).
2. Command returns `Err(AppError)`.
3. Tauri maps to `InvokeError` (string = friendly message).
4. `ipc.ts` → `toErrorMessage` → UI error banner (never a stack trace).

## Key decisions & why

| Decision | Rationale |
|---|---|
| Tauri 2 over Electron | Small binaries, native filesystem/integration, Rust for pixel work |
| SQLite (bundled rusqlite) | Zero servers, one file, perfect for statistics, survives OS-level inspection |
| Mutex-wrapped single connection | Simplest correct model for one-writer desktop catalog |
| Structured filter JSON | One representation reused by library, saved views, statistics |
| `algorithm_version` on analysis rows | Lets future algorithm improvements trigger re-analysis on demand |
| OS-conventional dirs | `~/.local/share`, `~/.cache`, `~/.local/state` (Linux) equivalents elsewhere |
| No routing library | 6 views, state-driven switching is simpler and faster to reason about |
