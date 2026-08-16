# RESUME NOTE (temporary — delete after Sprint 1 docs are written)

## Where we left off (Sprint 1 — Foundation)

**Status: functionally COMPLETE, awaiting final docs + commit close-out.**

| Check | Result |
|---|---|
| `cargo check --tests` | PASS |
| `cargo test` (2 tests: schema migrate + idempotency) | PASS |
| `npm run typecheck` | PASS |
| `npm test` (4 vitest tests) | PASS |
| `npm run build` (vite) | PASS (~54KB gzip) |
| `npm run build:app` (tauri debug bundle) | PASS — `.deb` produced at `src-tauri/target/debug/bundle/deb/PhotoGremlin_0.1.0_amd64.deb` |

## What exists now
- Full app shell: Tauri 2 + React 18 + TS + Vite; dark pro theme (`src/styles/theme.css`);
  sidebar nav (Library/Dashboard/Sessions/Collections/Saved Views/Settings);
  "Open Folder" flow via native dialog; Settings shows paths + privacy contract.
- Rust core: `commands/` (app_info, app_paths, db_status, pick_folder, set/get_active_folder),
  `database.rs` (SQLite, WAL, schema v1-v4 applied: photos, sessions, analysis, app_settings,
  collections, collection_photos, saved_views, similarity_groups(+photos), file_operations,
  schema_version), `error.rs` (AppError → InvokeError, friendly messages), `paths.rs`
  (OS data/cache/log dirs), `logging.rs` (rolling local log, no telemetry), `events.rs`
  (progress event names/payload — used from Sprint 2+), `state.rs`, `time.rs`.
- IPC surface typed end-to-end: `src/types/api.ts` mirrors Rust; `src/lib/ipc.ts` is the
  single invoke funnel; zustand store (`src/stores/appStore.ts`).
- Icon: `app-icon.png` (source) + `src-tauri/icons/*` (generated via `npx tauri icon`).
- AGENTS.md written. Tests: 2 Rust + 4 TS.

## First actions tomorrow
1. `source /home/shivam/pg-env.sh`  ← REQUIRED before any cargo/npm-tauri command
   (Rust on PATH + user-space webkit2gtk/gtk dev sysroot at ~/pgsysroot;
   no root available on this box, so system packages were extracted manually —
   the pg-env.sh script encodes PKG_CONFIG_PATH/LD_LIBRARY_PATH/CPATH).
2. Write the 13 docs in `docs/` (PRODUCT_SPEC, ARCHITECTURE, DATABASE, IMAGE_ANALYSIS,
   FILTER_ENGINE, STATISTICS, FILE_OPERATIONS, LOCAL_AI, PRIVACY, CROSS_PLATFORM,
   TESTING, DEVELOPMENT, ROADMAP) — describe what already exists for Sprint 1,
   mark later-sprint sections as planned. Then delete this file.
3. Optionally close Sprint 1 loose ends:
   - `.rpm` bundle fails without `rpmbuild` installed. Either `apt` it (needs root,
     unavailable) or set `bundle.targets` per OS (deb/appimage on Linux) — recommend
     Linux targets = `["deb", "appimage"]` once appimage tooling is available.
   - `AppAppPaths` etc. are fine as-is.
4. Start **Sprint 2 — Photo Scanner + Database ingestion**:
   - `scanner/` module: recursive walk (walkdir), supported extensions
     (jpg/jpeg/png/webp/tif/tiff now; RAW extensions detected+recorded but not
     decoded — provider interface note for docs/IMAGE_ANALYSIS.md),
     file stat (size, mtime), EXIF-light pass (orientation from w/h only at scan;
     full EXIF in Sprint 5), photo upsert with duplicate-path protection,
     session creation per scanned root (named after folder), progress events
     (`events::SCAN_PROGRESS`), cancellation token.
   - Commands: `scan_folder(path)`, `stop_scan()`.
   - LibraryView: wire Scan button + progress bar (store fields already exist).
   - Tests: synthetic images (write tiny JPG/PNG with `image` crate in a temp dir),
     scan → assert counts, rerun scan → idempotent (no dupes), unsupported files skipped.
   - Then per AGENTS.md: cargo test, npm test, typecheck, build:app, commit
     `feat: add photo scanner`.

## Key gotchas recorded
- `kamadak-exif = "0.6"` is the EXIF crate in this registry (the `exif` crate
  only lists 0.0.1 here). API: `exif::Reader::new().from_file(path)`,
  `info.get_field(tag, class)`, `field.rational()`.
- Tauri 2.11: `PathResolver<Wry>` (generic); `tauri::ipc::InvokeError`;
  `tracing_appender` builder `.build()` returns `Result`;
  use `EnvFilter::try_new` (not `try_from_str`).
- Linker fix: sysroot `.so` symlinks were dangling (runtime .so.0 live in
  /usr/lib); relinked 101+ to absolute system paths. If you re-extract
  debs into ~/pgsysroot, redo: for each dangling `*.so` symlink,
  `ln -sf /usr/lib/x86_64-linux-gnu/<target> <link>`.
- `sudo` requires a password (unavailable). User-space sysroot is the way;
  .debs cached in /tmp/opencode/debs (may be purged; regeneration recipe:
  `apt-get install -s <pkgs> | grep ^Inst`, `apt-get download`,
  `dpkg -x` into prefix, `sed` .pc files for the prefix).
- Keep builds incremental: full cold cargo build ≈ 15 min; incremental ≈ 20 s.
