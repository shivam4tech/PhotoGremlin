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
   │     analysis/    image measurement pipeline (background tasks)
   │     metadata/    EXIF extraction
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
- `state.rs` — `AppState` is an `Arc` of `Db` + `AppPaths`, shared via Tauri's
  managed state. Commands take `State<AppState>`.
- `database.rs` — single `Mutex<Connection>`; short critical sections only
  (never held across await). Versioned schema in `schema_version`
  (see DATABASE.md). WAL mode, foreign keys on.
- `error.rs` — one `AppError` enum. Its `Display` text is what the UI shows
  (friendly); `tracing::error!` inside constructors keeps the details in the
  local log. `From<AppError> for tauri::ipc::InvokeError` makes `Result<T,
  AppError>` a valid command return type.
- `events.rs` — IPC event names (`scan-progress`, `analysis-progress`,
  `db-changed`, `operation-progress`) + `ProgressPayload { total, done, stage,
  current }`. Progress flows Rust → UI via Tauri events, never by polling.
- `logging.rs` — `tracing` with a rolling daily file in the Tauri log dir
  (`<data_dir>/logs/photogremlin.<date>.log` on Linux) + console layer.
  Zero telemetry; the only sink is the local disk.
- `time.rs` — all stored timestamps are UTC RFC3339.

## Concurrency model

- Tauri runs commands on its thread pool; long-running work (scans, analysis,
  file ops — Sprints 2–7) runs as background tasks (tokio, already a
  dependency of Tauri) fed by channels, with bounded worker counts
  (decode/analyze at most a handful of full-res images in memory at once).
- The UI always stays responsive: nothing blocks on the webview thread, and
  progress events keep the spinner honest.
- SQLite: one writer at a time (the Mutex). Analysis writes batch by photo
  after each file, so incremental results appear while the pipeline runs.

## Frontend rules

- Views (`src/views/*`) render; features (`src/features/*`) hold logic;
  `stores/appStore.ts` (Zustand) holds app state.
- All backend calls go through `src/lib/ipc.ts` (typed, typed errors via
  `toErrorMessage`). No raw `invoke` anywhere in views.
- `src/types/api.ts` mirrors Rust serde types 1:1 — when a Rust type changes,
  the TS mirror changes in the same commit.
- Virtualized grid + cached thumbnails come in Sprint 3; the UI must never
  request full-resolution images for the grid.

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
