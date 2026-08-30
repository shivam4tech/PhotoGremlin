# Project catalogs and recovery (Sprint 27)

PhotoGremlin keeps runtime data local and opens one SQLite catalog per project.
Source photographs are never copied into a catalog or a catalog backup.

## Storage and switching

- `database.sqlite` is the global preferences and project-registry database.
  To avoid losing existing work, it also remains the catalog for whichever
  project was active when a pre-Sprint-27 installation first upgrades.
- Each subsequently opened canonical folder maps to
  `catalogs/<safe-folder-name>-<FNV-1a-of-canonical-path>.sqlite`. The mapping
  is stored under `project_catalog:<hash>` in the global database, so two
  folders with the same display name cannot collide.
- The replacement database opens, passes `PRAGMA integrity_check`, receives
  any schema migrations, and records its active root before `AppState` swaps
  the `{ Arc<Db>, path }` pair. Scans, analysis, metadata, similarity, local
  intelligence, exports, and file operations each retain their captured
  catalog `Arc` for the job lifetime. Project switching and recovery are
  refused while any of those jobs is running.

## Backups

Before migrating any existing catalog, PhotoGremlin creates a consistent
snapshot in `<data_dir>/catalog-backups/`. The Settings maintenance card can
also create one on demand. Backups use SQLite `VACUUM INTO`, not a copy of the
live main file without its WAL, and the operation refuses to overwrite an
existing path.

A backup contains indexed metadata, measurements, groups, review decisions,
collections, saved views, and the local file-operation audit. It contains no
photograph pixels, source files, training corpus, telemetry, or cloud data.

## Restore behavior

Settings lists the 20 newest `.sqlite` files inside PhotoGremlin's own backup
directory. Restore accepts no path outside that directory. It copies the
chosen backup to a new `catalogs/restored-…sqlite` file, opens and verifies
that copy, checks that its stored project matches the active project, then
updates the registry and switches catalogs. Neither the current catalog nor
the backup is overwritten, so a failed or unwanted recovery remains
reversible.

The health row in Settings runs SQLite's full integrity check and reports the
active schema version and path. Cache clearing is independent of catalog
recovery: it deletes only generated preview `.jpg`/`.part` files and never
touches photographs or database files.
