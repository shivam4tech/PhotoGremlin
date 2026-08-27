//! SQLite access. The catalog lives entirely on the user's machine.
//!
//! Concurrency model: a single `Connection` guarded by a `Mutex`. All
//! critical sections are short (no awaits while holding the lock).
//!
//! Schema is versioned via `schema_version` and applied incrementally.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{AppError, AppResult};

pub const CURRENT_SCHEMA_VERSION: i64 = 17;

/// Algorithm version for analysis results. Bump when analysis math changes.
pub const ANALYSIS_ALGORITHM_VERSION: i64 = 1;

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self, AppError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| AppError::io(e, path.display().to_string()))?;
        }
        let conn =
            Connection::open(path).map_err(|e| AppError::Database(e.to_string()))?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| AppError::Database(e.to_string()))?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| AppError::Database(e.to_string()))?;
        tracing::info!(path = %path.display(), "database opened");
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn migrate(&self) -> Result<i64, AppError> {
        let version = {
            let conn = self.lock()?;
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS schema_version (
                    version INTEGER NOT NULL,
                    applied_at TEXT NOT NULL
                 );",
            )
            .map_err(db_err("create schema_version"))?;

            // Core tables (v1).
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS sessions (
                    id INTEGER PRIMARY KEY,
                    name TEXT NOT NULL,
                    root_path TEXT,
                    start_time TEXT,
                    end_time TEXT,
                    photo_count INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL
                 );

                 CREATE TABLE IF NOT EXISTS photos (
                    id INTEGER PRIMARY KEY,
                    path TEXT NOT NULL UNIQUE,
                    filename TEXT NOT NULL,
                    extension TEXT NOT NULL,
                    size_bytes INTEGER,
                    width INTEGER,
                    height INTEGER,
                    orientation TEXT,
                    camera_make TEXT,
                    camera_model TEXT,
                    lens TEXT,
                    focal_length REAL,
                    iso INTEGER,
                    aperture REAL,
                    shutter_speed REAL,
                    capture_datetime TEXT,
                    gps_present INTEGER NOT NULL DEFAULT 0,
                    session_id INTEGER REFERENCES sessions(id) ON DELETE SET NULL,
                    indexed_at TEXT NOT NULL,
                    file_mtime TEXT
                 );

                 CREATE INDEX IF NOT EXISTS idx_photos_session ON photos(session_id);
                 CREATE INDEX IF NOT EXISTS idx_photos_capture ON photos(capture_datetime);
                 CREATE INDEX IF NOT EXISTS idx_photos_camera ON photos(camera_model);
                 CREATE INDEX IF NOT EXISTS idx_photos_lens ON photos(lens);

                 CREATE TABLE IF NOT EXISTS analysis (
                    photo_id INTEGER PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
                    sharpness REAL,
                    brightness REAL,
                    contrast REAL,
                    saturation REAL,
                    highlight_clipping REAL,
                    shadow_clipping REAL,
                    is_monochrome INTEGER NOT NULL DEFAULT 0,
                    is_dark INTEGER NOT NULL DEFAULT 0,
                    is_bright INTEGER NOT NULL DEFAULT 0,
                    face_count INTEGER,
                    smile_count INTEGER,
                    perceptual_hash TEXT,
                    algorithm_version INTEGER NOT NULL DEFAULT 1,
                    analyzed_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS idx_analysis_sharpness ON analysis(sharpness);
                 CREATE INDEX IF NOT EXISTS idx_analysis_perceptual ON analysis(perceptual_hash);

                 CREATE TABLE IF NOT EXISTS app_settings (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );",
            )
            .map_err(db_err("create core tables"))?;

            // v2: collections (manually curated sets).
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS collections (
                    id INTEGER PRIMARY KEY,
                    name TEXT NOT NULL UNIQUE,
                    description TEXT,
                    created_at TEXT NOT NULL
                 );

                 CREATE TABLE IF NOT EXISTS collection_photos (
                    collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
                    photo_id INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
                    added_at TEXT NOT NULL,
                    PRIMARY KEY (collection_id, photo_id)
                 );",
            )
            .map_err(db_err("create collections"))?;

            // v3: saved views (dynamic filters) + similarity groups.
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS saved_views (
                    id INTEGER PRIMARY KEY,
                    name TEXT NOT NULL UNIQUE,
                    filter_json TEXT NOT NULL,
                    description TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );

                 CREATE TABLE IF NOT EXISTS similarity_groups (
                    id INTEGER PRIMARY KEY,
                    hash TEXT NOT NULL,
                    group_type TEXT NOT NULL DEFAULT 'similar',
                    photo_count INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS idx_similarity_groups_hash ON similarity_groups(hash);

                 CREATE TABLE IF NOT EXISTS similarity_group_photos (
                    group_id INTEGER NOT NULL REFERENCES similarity_groups(id) ON DELETE CASCADE,
                    photo_id INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
                    PRIMARY KEY (group_id, photo_id)
                 );",
            )
            .map_err(db_err("create views + similarity"))?;

            // v4: file operation audit log (rename/move/copy/trash history).
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS file_operations (
                    id INTEGER PRIMARY KEY,
                    op_type TEXT NOT NULL,
                    source_path TEXT NOT NULL,
                    dest_path TEXT,
                    status TEXT NOT NULL,
                    detail TEXT,
                    created_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS idx_file_operations_created ON file_operations(created_at);",
            )
            .map_err(db_err("create file_operations"))?;

            // v5: one session per imported folder (root_path). Manual sessions
            // keep root_path NULL (multiple allowed).
            conn.execute_batch(
                "CREATE UNIQUE INDEX IF NOT EXISTS idx_sessions_root
                 ON sessions(root_path) WHERE root_path IS NOT NULL;",
            )
            .map_err(db_err("create sessions root index"))?;

            // v6: record the source file mtime each analysis row was computed
            // from, so re-analysis is incremental (only re-measure when the
            // file changed on disk or the algorithm version advanced).
            if !table_has_column(&conn, "analysis", "source_mtime") {
                conn.execute("ALTER TABLE analysis ADD COLUMN source_mtime TEXT", [])
                    .map_err(db_err("add analysis.source_mtime"))?;
            }

            // v7: record when the EXIF/metadata pass last touched a photo
            // (RFC3339). Drives the "metadata pending" count and lets the
            // pass skip files it has already read.
            if !table_has_column(&conn, "photos", "exif_at") {
                conn.execute("ALTER TABLE photos ADD COLUMN exif_at TEXT", [])
                    .map_err(db_err("add photos.exif_at"))?;
            }

            // v8: explicit selection state (culling: keep/reject). The
            // selection UI lands in Sprint 7; the statistics engine's
            // selection-ratio query reads this (and file_operations) now.
            conn.execute(
                "CREATE TABLE IF NOT EXISTS selections (
                    photo_id INTEGER PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
                    state TEXT NOT NULL CHECK (state IN ('selected', 'rejected')),
                    updated_at TEXT
                 );",
                [],
            )
            .map_err(db_err("create selections"))?;

            // v9: perceptual-hash columns for the similarity pass (Sprint 8).
            // `phash_source_mtime` records the file mtime the hash was computed
            // for, so a modified file is re-hashed on the next pass (same
            // incremental rule as analysis).
            if !table_has_column(&conn, "photos", "phash") {
                conn.execute("ALTER TABLE photos ADD COLUMN phash INTEGER", [])
                    .map_err(db_err("add photos.phash"))?;
            }
            if !table_has_column(&conn, "photos", "phash_source_mtime") {
                conn.execute("ALTER TABLE photos ADD COLUMN phash_source_mtime TEXT", [])
                    .map_err(db_err("add photos.phash_source_mtime"))?;
            }

            // v10: face-detection stamp for the local-AI pass (Sprint 9).
            // `faces_at` records the file mtime when faces were last
            // detected (re-detect when the file gets newer), mirroring
            // `phash_source_mtime`. Lives with `face_count` on `analysis`;
            // a face-only row (no measurements yet) is created by the face
            // pass and filled in by the analysis pass later.
            if !table_has_column(&conn, "analysis", "faces_at") {
                conn.execute("ALTER TABLE analysis ADD COLUMN faces_at TEXT", [])
                    .map_err(db_err("add analysis.faces_at"))?;
            }

            // v11 (Sprint 11): metadata reliability. `lens_make` / `software`
            // extend what the EXIF pass can contribute; `metadata_source`
            // records where a photo's camera/exposure/date values came from
            // ('none' until the pass finds something: 'exif' when real EXIF
            // contributes; the date-estimation sprint adds 'filename' /
            // 'mtime' below 'exif' in the dominance order). The EXIF queue
            // also becomes incremental: a file whose mtime is newer than its
            // last read (`file_mtime > exif_at`) is re-read, mirroring the
            // analysis/similarity/#faces rules.
            if !table_has_column(&conn, "photos", "lens_make") {
                conn.execute("ALTER TABLE photos ADD COLUMN lens_make TEXT", [])
                    .map_err(db_err("add photos.lens_make"))?;
            }
            if !table_has_column(&conn, "photos", "software") {
                conn.execute("ALTER TABLE photos ADD COLUMN software TEXT", [])
                    .map_err(db_err("add photos.software"))?;
            }
            if !table_has_column(&conn, "photos", "metadata_source") {
                conn.execute(
                    "ALTER TABLE photos ADD COLUMN metadata_source TEXT NOT NULL DEFAULT 'none'",
                    [],
                )
                .map_err(db_err("add photos.metadata_source"))?;
            }

            // v12 (Sprint 12): date estimation. `capture_datetime_source`
            // records the provenance of the capture datetime itself
            // ('exif' | 'filename' | 'mtime'), so the UI can label an
            // estimate instead of pretending precision. Backfill: before
            // estimation existed, every stored capture datetime came from
            // EXIF, so that label is safe for existing rows.
            if !table_has_column(&conn, "photos", "capture_datetime_source") {
                conn.execute(
                    "ALTER TABLE photos ADD COLUMN capture_datetime_source TEXT",
                    [],
                )
                .map_err(db_err("add photos.capture_datetime_source"))?;
                conn.execute(
                    "UPDATE photos SET capture_datetime_source = 'exif'
                     WHERE capture_datetime IS NOT NULL AND capture_datetime_source IS NULL",
                    [],
                )
                .map_err(db_err("backfill capture_datetime_source"))?;
            }

            // v13 (Sprint 13): curatorial marks — ratings, flags and color
            // labels. All default to "unmarked": rating NULL = unrated
            // (0..5 when set), flag 0/1, color_label NULL or one of the
            // fixed enum values below. No backfill needed.
            if !table_has_column(&conn, "photos", "rating") {
                conn.execute("ALTER TABLE photos ADD COLUMN rating INTEGER", [])
                    .map_err(db_err("add photos.rating"))?;
            }
            if !table_has_column(&conn, "photos", "flag") {
                conn.execute(
                    "ALTER TABLE photos ADD COLUMN flag INTEGER NOT NULL DEFAULT 0",
                    [],
                )
                .map_err(db_err("add photos.flag"))?;
            }
            if !table_has_column(&conn, "photos", "color_label") {
                conn.execute("ALTER TABLE photos ADD COLUMN color_label TEXT", [])
                    .map_err(db_err("add photos.color_label"))?;
            }

            // v14 (Sprint 18): scene-classification results from the local
            // scene model (optional ml layer, mirrors `faces_at`). `coarse`
            // stores the MERGED product group (10 chips), `fine` the scene
            // label, `conf` its softmax confidence; NULL until the pass runs
            // and every core feature works with the model absent.
            if !table_has_column(&conn, "analysis", "scene_coarse") {
                conn.execute("ALTER TABLE analysis ADD COLUMN scene_coarse TEXT", [])
                    .map_err(db_err("add analysis.scene_coarse"))?;
            }
            if !table_has_column(&conn, "analysis", "scene_fine") {
                conn.execute("ALTER TABLE analysis ADD COLUMN scene_fine TEXT", [])
                    .map_err(db_err("add analysis.scene_fine"))?;
            }
            if !table_has_column(&conn, "analysis", "scene_conf") {
                conn.execute("ALTER TABLE analysis ADD COLUMN scene_conf REAL", [])
                    .map_err(db_err("add analysis.scene_conf"))?;
            }
            if !table_has_column(&conn, "analysis", "scene_at") {
                conn.execute("ALTER TABLE analysis ADD COLUMN scene_at TEXT", [])
                    .map_err(db_err("add analysis.scene_at"))?;
            }

            // v15 (Review workflow): selection remains the one durable,
            // non-destructive review state. Rebuild its small CHECK
            // constraint to add `needs_attention`; existing keep/reject
            // rows are copied verbatim. SQLite cannot alter CHECK clauses.
            let selection_sql: Option<String> = conn
                .query_row(
                    "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'selections'",
                    [],
                    |r| r.get(0),
                )
                .optional()
                .map_err(db_err("read selections schema"))?;
            if !selection_sql
                .as_deref()
                .unwrap_or_default()
                .contains("needs_attention")
            {
                conn.execute_batch(
                    "CREATE TABLE selections_v15 (
                        photo_id INTEGER PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
                        state TEXT NOT NULL CHECK (state IN ('selected', 'rejected', 'needs_attention')),
                        updated_at TEXT
                     );
                     INSERT INTO selections_v15 (photo_id, state, updated_at)
                        SELECT photo_id, state, updated_at FROM selections;
                     DROP TABLE selections;
                     ALTER TABLE selections_v15 RENAME TO selections;",
                )
                .map_err(db_err("upgrade selections review states"))?;
            }

            // v16: similarity results are durable per project/session. Older
            // versions replaced one global result set and cleared it when the
            // project changed, which made cards from another project appear
            // (or disappear). Keep groups whose members all belong to one
            // session; legacy cross-session groups have no honest project
            // owner and are discarded on this one-time migration.
            if !table_has_column(&conn, "similarity_groups", "session_id") {
                conn.execute(
                    "ALTER TABLE similarity_groups ADD COLUMN session_id INTEGER",
                    [],
                )
                .map_err(db_err("add similarity_groups.session_id"))?;
                conn.execute_batch(
                    "UPDATE similarity_groups
                        SET session_id = (
                            SELECT p.session_id
                            FROM similarity_group_photos gp
                            JOIN photos p ON p.id = gp.photo_id
                            WHERE gp.group_id = similarity_groups.id
                            GROUP BY p.session_id
                            HAVING COUNT(DISTINCT COALESCE(p.session_id, -1)) = 1
                            LIMIT 1
                        );
                     DELETE FROM similarity_group_photos
                       WHERE group_id IN (
                           SELECT id FROM similarity_groups WHERE session_id IS NULL
                       );
                     DELETE FROM similarity_groups WHERE session_id IS NULL;
                     CREATE INDEX IF NOT EXISTS idx_similarity_groups_session
                       ON similarity_groups(session_id);",
                )
                .map_err(db_err("scope legacy similarity groups"))?;
            }

            // v17: face observations hold only compact, local appearance
            // hashes for detected crops. They let the similarity pass offer
            // same-face candidates without a cloud identity service or a
            // heavyweight recognition model. No image pixels leave the
            // machine, and a missing face runtime simply leaves this table
            // empty.
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS face_observations (
                    photo_id INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
                    face_index INTEGER NOT NULL,
                    appearance_hash INTEGER NOT NULL,
                    source_mtime TEXT,
                    PRIMARY KEY (photo_id, face_index)
                 );
                 CREATE INDEX IF NOT EXISTS idx_face_observations_hash
                    ON face_observations(appearance_hash);",
            )
            .map_err(db_err("create face observations"))?;

            let current_version: i64 = conn
                .query_row("SELECT COALESCE(MAX(version), 0) FROM schema_version", [], |r| {
                    r.get(0)
                })
                .optional()
                .map_err(db_err("read schema version"))?
                .unwrap_or(0);

            let mut now = crate::time::now_utc();
            let target = CURRENT_SCHEMA_VERSION;
            for v in (current_version + 1)..=target {
                conn.execute(
                    "INSERT INTO schema_version (version, applied_at) VALUES (?1, ?2)",
                    params![v, &now],
                )
                .map_err(db_err("record schema version"))?;
                now = crate::time::now_utc();
            }
            tracing::info!(from = current_version, to = target, "schema migrated");
            target
        };
        Ok(version)
    }

    fn active_session_id(&self, conn: &Connection) -> Result<Option<i64>, AppError> {
        let active: Option<String> = conn
            .query_row(
                "SELECT value FROM app_settings WHERE key = 'active_folder'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_err("read active_folder"))?;
        if let Some(path) = active {
            let sid: Option<i64> = conn
                .query_row(
                    "SELECT id FROM sessions WHERE root_path = ?1",
                    params![path],
                    |r| r.get(0),
                )
                .optional()
                .map_err(db_err("lookup active session"))?;
            Ok(sid)
        } else {
            Ok(None)
        }
    }

    /// The session for the folder currently open in the desktop app. Long
    /// running project work must capture this before it starts rather than
    /// falling back to the entire catalog.
    pub fn active_project_session_id(&self) -> AppResult<Option<i64>> {
        let conn = self.lock()?;
        self.active_session_id(&conn)
    }

    /// Quick status counters for the shell UI. When a project is open
    /// (active_folder → session), counts are scoped to that session so the
    /// TopBar badge, dashboard, and progress all reflect the current project,
    /// not a stale global total. With no project (Home), global counts are
    /// returned so the dashboard still has data.
    pub fn status(&self) -> Result<DbStatus, AppError> {
        let conn = self.lock()?;
        let active_sid = self.active_session_id(&conn)?;
        let photo_count: i64 = match active_sid {
            Some(sid) => conn
                .query_row("SELECT COUNT(*) FROM photos WHERE session_id = ?1", params![sid], |r| r.get(0))
                .map_err(db_err("count photos"))?,
            None => conn
                .query_row("SELECT COUNT(*) FROM photos", [], |r| r.get(0))
                .map_err(db_err("count photos"))?,
        };
        let session_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .map_err(db_err("count sessions"))?;
        let analyzed_count: i64 = match active_sid {
            Some(sid) => conn
                .query_row(
                    "SELECT COUNT(*) FROM analysis a JOIN photos p ON p.id = a.photo_id WHERE p.session_id = ?1",
                    params![sid],
                    |r| r.get(0),
                )
                .map_err(db_err("count analysis"))?,
            None => conn
                .query_row("SELECT COUNT(*) FROM analysis", [], |r| r.get(0))
                .map_err(db_err("count analysis"))?,
        };
        // Photos still awaiting the EXIF/metadata pass (never read yet, or
        // changed on disk since the last read) — scoped to active session.
        let metadata_pending: i64 = match active_sid {
            Some(sid) => conn
                .query_row(
                    "SELECT COUNT(*) FROM photos
                      WHERE session_id = ?1 AND (
                        exif_at IS NULL
                        OR (file_mtime IS NOT NULL AND exif_at IS NOT NULL AND file_mtime > exif_at)
                      )",
                    params![sid],
                    |r| r.get(0),
                )
                .map_err(db_err("count metadata pending"))?,
            None => conn
                .query_row(
                    "SELECT COUNT(*) FROM photos
                      WHERE exif_at IS NULL
                         OR (file_mtime IS NOT NULL AND exif_at IS NOT NULL AND file_mtime > exif_at)",
                    [],
                    |r| r.get(0),
                )
                .map_err(db_err("count metadata pending"))?,
        };
        let (selected_count, rejected_count): (i64, i64) = match active_sid {
            Some(sid) => conn
                .query_row(
                    "SELECT
                       (SELECT COUNT(*) FROM selections s JOIN photos p ON p.id = s.photo_id WHERE s.state = 'selected' AND p.session_id = ?1),
                       (SELECT COUNT(*) FROM selections s JOIN photos p ON p.id = s.photo_id WHERE s.state = 'rejected' AND p.session_id = ?1)",
                    params![sid],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .map_err(db_err("count selections"))?,
            None => conn
                .query_row(
                    "SELECT
                       (SELECT COUNT(*) FROM selections WHERE state = 'selected'),
                       (SELECT COUNT(*) FROM selections WHERE state = 'rejected')",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .map_err(db_err("count selections"))?,
        };
        let version: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_version",
                [],
                |r| r.get(0),
            )
            .map_err(db_err("read version"))?;
        let faces_done: i64 = match active_sid {
            Some(sid) => conn
                .query_row(
                    "SELECT COUNT(*) FROM analysis a JOIN photos p ON p.id = a.photo_id WHERE a.face_count IS NOT NULL AND p.session_id = ?1",
                    params![sid],
                    |r| r.get(0),
                )
                .map_err(db_err("count faces done"))?,
            None => conn
                .query_row(
                    "SELECT COUNT(*) FROM analysis WHERE face_count IS NOT NULL",
                    [],
                    |r| r.get(0),
                )
                .map_err(db_err("count faces done"))?,
        };
        let scenes_done: i64 = match active_sid {
            Some(sid) => conn
                .query_row(
                    "SELECT COUNT(*) FROM analysis a JOIN photos p ON p.id = a.photo_id WHERE a.scene_fine IS NOT NULL AND p.session_id = ?1",
                    params![sid],
                    |r| r.get(0),
                )
                .map_err(db_err("count scenes done"))?,
            None => conn
                .query_row(
                    "SELECT COUNT(*) FROM analysis WHERE scene_fine IS NOT NULL",
                    [],
                    |r| r.get(0),
                )
                .map_err(db_err("count scenes done"))?,
        };
        Ok(DbStatus {
            photo_count,
            session_count,
            analyzed_count,
            metadata_pending,
            selected_count,
            rejected_count,
            faces_done,
            scenes_done,
            schema_version: version,
        })
    }

    pub fn set_setting(&self, key: &str, value: &str) -> AppResult<()> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO app_settings (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, value, crate::time::now_utc()],
        )
        .map_err(db_err("set setting"))?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> AppResult<Option<String>> {
        let conn = self.lock()?;
        let v: Option<String> = conn
            .query_row("SELECT value FROM app_settings WHERE key = ?1", params![key], |r| {
                r.get(0)
            })
            .optional()
            .map_err(db_err("get setting"))?;
        Ok(v)
    }

    pub fn clear_setting(&self, key: &str) -> AppResult<()> {
        let conn = self.lock()?;
        conn.execute("DELETE FROM app_settings WHERE key = ?1", params![key])
            .map_err(db_err("clear setting"))?;
        Ok(())
    }

    /// Upsert a session by its import root. Returns the session id.
    /// One session per folder (enforced by the v5 partial unique index);
    /// a re-scan of the same folder keeps the same session (name refreshes).
    pub fn upsert_session(
        &self,
        name: &str,
        root_path: Option<&str>,
    ) -> AppResult<i64> {
        let conn = self.lock()?;
        if let Some(root) = root_path {
            let existing: Option<i64> = conn
                .query_row(
                    "SELECT id FROM sessions WHERE root_path = ?1",
                    params![root],
                    |r| r.get(0),
                )
                .optional()
                .map_err(db_err("find session"))?;
            if let Some(id) = existing {
                conn.execute(
                    "UPDATE sessions SET name = ?2 WHERE id = ?1",
                    params![id, name],
                )
                .map_err(db_err("refresh session name"))?;
                return Ok(id);
            }
        }
        let name_owned = name.to_string();
        let root_owned = root_path.map(|s| s.to_string());
        conn.execute(
            "INSERT INTO sessions (name, root_path, created_at) VALUES (?1, ?2, ?3)",
            params![name_owned, root_owned, crate::time::now_utc()],
        )
        .map_err(db_err("insert session"))?;
        Ok(conn.last_insert_rowid())
    }

    pub fn session_by_id(&self, id: i64) -> AppResult<Option<SessionRow>> {
        let conn = self.lock()?;
        let row: Option<SessionRow> = conn
            .query_row(
                "SELECT id, name, root_path, start_time, end_time, photo_count, created_at
                 FROM sessions WHERE id = ?1",
                params![id],
                |r| {
                    Ok(SessionRow {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        root_path: r.get(2)?,
                        start_time: r.get(3)?,
                        end_time: r.get(4)?,
                        photo_count: r.get(5)?,
                        created_at: r.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(db_err("session by id"))?;
        Ok(row)
    }

    pub fn list_sessions(&self) -> AppResult<Vec<SessionRow>> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, root_path, start_time, end_time, photo_count, created_at
                 FROM sessions ORDER BY created_at DESC",
            )
            .map_err(db_err("prepare sessions"))?;
        let rows = stmt
            .query_map(
                    [],
                    |r| {
                Ok(SessionRow {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    root_path: r.get(2)?,
                    start_time: r.get(3)?,
                    end_time: r.get(4)?,
                    photo_count: r.get(5)?,
                    created_at: r.get(6)?,
                })
            })
            .map_err(db_err("query sessions"))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(db_err("read session row"))?);
        }
        Ok(out)
    }

    /// Insert-or-update a photo by path. Re-scans refresh size/mtime and
    /// dimensions, but never blank out values a later pipeline filled in
    /// (COALESCE) and never rewrite `indexed_at`.
    pub fn upsert_photo(&self, p: &PhotoUpsert) -> AppResult<i64> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO photos (path, filename, extension, size_bytes, width, height,
                                 orientation, session_id, indexed_at, file_mtime)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(path) DO UPDATE SET
               size_bytes  = excluded.size_bytes,
               file_mtime  = excluded.file_mtime,
               width       = COALESCE(excluded.width, photos.width),
               height      = COALESCE(excluded.height, photos.height),
               orientation = COALESCE(excluded.orientation, photos.orientation),
               session_id  = COALESCE(excluded.session_id, photos.session_id)",
            params![
                p.path,
                p.filename,
                p.extension,
                p.size_bytes,
                p.width,
                p.height,
                p.orientation,
                p.session_id,
                crate::time::now_utc(),
                p.file_mtime,
            ],
        )
        .map_err(db_err("upsert photo"))?;

        // Recover the id (last_insert_rowid() is not reliable on update).
        let id: i64 = conn
            .query_row(
                "SELECT id FROM photos WHERE path = ?1",
                params![p.path],
                |r| r.get(0),
            )
            .map_err(db_err("photo id lookup"))?;
        Ok(id)
    }

    /// Refresh a session's denormalized counters after a scan pass.
    /// Refresh one session's denormalized counters: photo_count plus
    /// start/end of the shoot, derived from the photos' best-known time
    /// (`COALESCE(capture_datetime, indexed_at)` — EXIF fills the former as
    /// the metadata pass runs). Empty sessions get NULL times.
    pub fn refresh_session_counts(&self, session_id: i64) -> AppResult<()> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE sessions SET
                 photo_count = (SELECT COUNT(*) FROM photos WHERE session_id = ?1),
                 start_time  = (SELECT MIN(COALESCE(capture_datetime, indexed_at))
                                FROM photos WHERE session_id = ?1
                                AND COALESCE(capture_datetime, indexed_at) IS NOT NULL),
                 end_time    = (SELECT MAX(COALESCE(capture_datetime, indexed_at))
                                FROM photos WHERE session_id = ?1
                                AND COALESCE(capture_datetime, indexed_at) IS NOT NULL)
             WHERE id = ?1",
            params![session_id],
        )
        .map_err(db_err("refresh session counts"))?;
        Ok(())
    }

    /// Same refresh for every session in one statement — used after the
    /// metadata pass, which is what fills capture datetimes.
    pub fn refresh_all_sessions_times(&self) -> AppResult<()> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE sessions SET
                 photo_count = (SELECT COUNT(*) FROM photos WHERE session_id = sessions.id),
                 start_time  = (SELECT MIN(COALESCE(p.capture_datetime, p.indexed_at))
                                FROM photos p WHERE p.session_id = sessions.id
                                AND COALESCE(p.capture_datetime, p.indexed_at) IS NOT NULL),
                 end_time    = (SELECT MAX(COALESCE(p.capture_datetime, p.indexed_at))
                                FROM photos p WHERE p.session_id = sessions.id
                                AND COALESCE(p.capture_datetime, p.indexed_at) IS NOT NULL)",
            [],
        )
        .map_err(db_err("refresh all session times"))?;
        Ok(())
    }

    /// One full photo with its analysis row (for the viewer metadata panel).
    pub fn get_photo_full(&self, id: i64) -> AppResult<PhotoFull> {
        let conn = self.lock()?;
        let row: Option<PhotoFull> = conn
            .query_row(
                "SELECT p.id, p.path, p.filename, p.extension, p.size_bytes, p.width, p.height,
                        p.orientation, p.camera_make, p.camera_model, p.lens,
                        p.lens_make, p.software, p.metadata_source, p.capture_datetime_source,
                        p.focal_length,
                        p.iso, p.aperture, p.shutter_speed, p.capture_datetime, p.gps_present,
                        p.session_id, p.rating, p.flag, p.color_label,
                        p.indexed_at, p.file_mtime,
                        a.sharpness, a.brightness, a.contrast, a.saturation,
                        a.highlight_clipping, a.shadow_clipping, a.is_monochrome, a.is_dark,
                        a.is_bright, a.face_count, a.smile_count,
                        a.scene_coarse, a.scene_fine, a.scene_conf,
                        a.perceptual_hash, a.algorithm_version, a.analyzed_at
                 FROM photos p
                 LEFT JOIN analysis a ON a.photo_id = p.id
                 WHERE p.id = ?1",
                params![id],
                |r| {
                    Ok(PhotoFull {
                        id: r.get(0)?,
                        path: r.get(1)?,
                        filename: r.get(2)?,
                        extension: r.get(3)?,
                        size_bytes: r.get(4)?,
                        width: r.get(5)?,
                        height: r.get(6)?,
                        orientation: r.get(7)?,
                        camera_make: r.get(8)?,
                        camera_model: r.get(9)?,
                        lens: r.get(10)?,
                        lens_make: r.get(11)?,
                        software: r.get(12)?,
                        metadata_source: r.get(13)?,
                        capture_datetime_source: r.get(14)?,
                        focal_length: r.get(15)?,
                        iso: r.get(16)?,
                        aperture: r.get(17)?,
                        shutter_speed: r.get(18)?,
                        capture_datetime: r.get(19)?,
                        gps_present: r.get::<_, i64>(20)? != 0,
                        session_id: r.get(21)?,
                        rating: r.get(22)?,
                        flag: r.get::<_, i64>(23)? != 0,
                        color_label: r.get(24)?,
                        indexed_at: r.get(25)?,
                        file_mtime: r.get(26)?,
                        sharpness: r.get(27)?,
                        brightness: r.get(28)?,
                        contrast: r.get(29)?,
                        saturation: r.get(30)?,
                        highlight_clipping: r.get(31)?,
                        shadow_clipping: r.get(32)?,
                        is_monochrome: r.get::<_, Option<i64>>(33)?.unwrap_or(0) != 0,
                        is_dark: r.get::<_, Option<i64>>(34)?.unwrap_or(0) != 0,
                        is_bright: r.get::<_, Option<i64>>(35)?.unwrap_or(0) != 0,
                        face_count: r.get(36)?,
                        smile_count: r.get(37)?,
                        scene_coarse: r.get(38)?,
                        scene_fine: r.get(39)?,
                        scene_conf: r.get(40)?,
                        perceptual_hash: r.get(41)?,
                        algorithm_version: r.get(42)?,
                        analyzed_at: r.get(43)?,
                    })
                },
            )
            .optional()
            .map_err(db_err("get_photo_full"))?;
        row.ok_or_else(|| AppError::operation("This photograph is no longer in the library"))
    }

    /// Persist one analyzed photo's metrics (Sprint 4). Idempotent by
    /// `photo_id` (the analysis PK) — re-analysis overwrites the row, bumps
    /// `analyzed_at`, and records the `source_mtime` the values were computed
    /// from so the next run can skip unchanged files.
    pub fn upsert_analysis(
        &self,
        photo_id: i64,
        m: &crate::analysis::metrics::Metrics,
        source_mtime: Option<&str>,
    ) -> AppResult<()> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO analysis (
                photo_id, sharpness, brightness, contrast, saturation,
                highlight_clipping, shadow_clipping, is_monochrome, is_dark, is_bright,
                algorithm_version, analyzed_at, source_mtime
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(photo_id) DO UPDATE SET
                sharpness = excluded.sharpness,
                brightness = excluded.brightness,
                contrast = excluded.contrast,
                saturation = excluded.saturation,
                highlight_clipping = excluded.highlight_clipping,
                shadow_clipping = excluded.shadow_clipping,
                is_monochrome = excluded.is_monochrome,
                is_dark = excluded.is_dark,
                is_bright = excluded.is_bright,
                algorithm_version = excluded.algorithm_version,
                analyzed_at = excluded.analyzed_at,
                source_mtime = excluded.source_mtime",
            params![
                photo_id,
                m.sharpness,
                m.brightness,
                m.contrast,
                m.saturation,
                m.highlight_clipping,
                m.shadow_clipping,
                i64::from(m.is_monochrome),
                i64::from(m.is_dark),
                i64::from(m.is_bright),
                ANALYSIS_ALGORITHM_VERSION,
                crate::time::now_utc(),
                source_mtime,
            ],
        )
        .map_err(db_err("upsert_analysis"))?;
        Ok(())
    }

    /// Photos still needing analysis: decodable pixels, and either no row yet,
    /// a row from an older algorithm, or a file whose mtime changed after the
    /// row was written. Order is stable (capture time, then id) so the UI and
    /// a resumed run see the same sequence.
    pub fn analysis_queue(&self, extensions: &[&str]) -> AppResult<Vec<AnalysisWork>> {
        let conn = self.lock()?;
        let active_sid = self.active_session_id(&conn)?;
        let exts: Vec<String> = extensions.iter().map(|s| s.to_string()).collect();
        // ?1 = current algorithm version, ?2.. = the decodable extension list.
        let placeholders = (2..=exts.len() + 1)
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let mut sql = String::from(
            "SELECT p.id, p.path, p.file_mtime, p.filename
             FROM photos p
             LEFT JOIN analysis a ON a.photo_id = p.id
             WHERE p.extension IN (",
        );
        sql.push_str(&placeholders);
        sql.push_str(
            ") AND (
                 a.photo_id IS NULL
                 OR a.algorithm_version < ?1
                 OR a.source_mtime IS NOT p.file_mtime)",
        );
        if active_sid.is_some() {
            sql.push_str(" AND p.session_id = ?");
            sql.push_str(&(exts.len() + 2).to_string());
        }
        sql.push_str(" ORDER BY p.capture_datetime IS NULL ASC, p.capture_datetime ASC, p.id ASC");
        let mut stmt = conn.prepare(&sql).map_err(db_err("prepare analysis_queue"))?;
        let mut params: Vec<Box<dyn rusqlite::ToSql>> =
            vec![Box::new(ANALYSIS_ALGORITHM_VERSION)];
        for e in &exts {
            params.push(Box::new(e.clone()));
        }
        if let Some(sid) = active_sid {
            params.push(Box::new(sid));
        }
        let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
        let rows = stmt
            .query_map(refs.as_slice(), |r| {
                Ok(AnalysisWork {
                    photo_id: r.get(0)?,
                    path: r.get(1)?,
                    file_mtime: r.get(2)?,
                    filename: r.get(3)?,
                })
            })
            .map_err(db_err("query analysis_queue"))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(db_err("read analysis row"))?);
        }
        Ok(out)
    }

    /// (decodable photos, analyzed-of-them) for the "Analyzed N of M" status
    /// line and the analyze button's enablement.
    pub fn analysis_progress_counts(&self, extensions: &[&str]) -> AppResult<(i64, i64)> {
        let conn = self.lock()?;
        let exts: Vec<String> = extensions.iter().map(|s| s.to_string()).collect();
        let placeholders = (1..=exts.len())
            .map(|i| format!("?{}", i))
            .collect::<Vec<_>>()
            .join(", ");
        let mut sql = String::from("SELECT COUNT(*) FROM photos p WHERE p.extension IN (");
        sql.push_str(&placeholders);
        sql.push_str(")");
        let mut stmt = conn.prepare(&sql).map_err(db_err("prepare decodable count"))?;
        let refs: Vec<Box<dyn rusqlite::ToSql>> =
            exts.iter().map(|e| Box::new(e.clone()) as Box<dyn rusqlite::ToSql>).collect();
        let rrefs: Vec<&dyn rusqlite::ToSql> = refs.iter().map(|b| b.as_ref()).collect();
        let decodable: i64 = stmt
            .query_row(rrefs.as_slice(), |r| r.get(0))
            .map_err(db_err("count decodable"))?;

        let mut sql2 = String::from(
            "SELECT COUNT(*) FROM photos p JOIN analysis a ON a.photo_id = p.id
             WHERE p.extension IN (",
        );
        sql2.push_str(&placeholders);
        sql2.push_str(")");
        let mut stmt2 = conn.prepare(&sql2).map_err(db_err("prepare analyzed count"))?;
        let analyzed: i64 = stmt2
            .query_row(rrefs.as_slice(), |r| r.get(0))
            .map_err(db_err("count analyzed"))?;
        Ok((decodable, analyzed))
    }

    /// Photos still awaiting the EXIF/metadata pass for the open project, in
    /// stable order: never read yet, or changed on disk since the last read
    /// (incremental rule: `file_mtime > exif_at` — same idea as the
    /// analysis/similarity/faces queues). An unchanged file never re-enters.
    /// With no open project, Home retains the useful global queue.
    pub fn exif_queue(&self) -> AppResult<Vec<ExifWork>> {
        let conn = self.lock()?;
        let active_sid = self.active_session_id(&conn)?;
        let sql = "SELECT id, path, extension, filename, width, height, orientation, file_mtime
                   FROM photos
                   WHERE (?1 IS NULL OR session_id = ?1)
                     AND (
                       exif_at IS NULL
                       OR (file_mtime IS NOT NULL AND exif_at IS NOT NULL AND file_mtime > exif_at)
                     )
                   ORDER BY (capture_datetime IS NULL) ASC, capture_datetime ASC, id ASC";
        let mut stmt = conn.prepare(sql).map_err(db_err("prepare exif_queue"))?;
        let rows = stmt
            .query_map(params![active_sid], |r| {
                Ok(ExifWork {
                    photo_id: r.get(0)?,
                    path: r.get(1)?,
                    extension: r.get(2)?,
                    filename: r.get(3)?,
                    width: r.get(4)?,
                    height: r.get(5)?,
                    orientation: r.get(6)?,
                    file_mtime: r.get(7)?,
                })
            })
            .map_err(db_err("query exif_queue"))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(db_err("read exif work row"))?);
        }
        Ok(out)
    }

/// Persist one photo's EXIF/metadata extraction (+ capture-date estimation,
/// Sprint 12). Merge semantics:
/// - scanner-resolved dimensions/orientation always win (COALESCE keeps the
///   existing value — the scanner has authoritative pixels);
/// - EXIF-owned fields are refreshed by the newest read: a non-None
///   value replaces the old one (so a re-read of a changed file actually
///   updates the row), while a None in a fresh read preserves what an
///   earlier read found (an empty read never erases data);
/// - capture datetime: real EXIF beats the estimate beats the stored value;
///   its provenance is recorded (`capture_datetime_source`: 'exif' |
///   'filename' | 'mtime') so estimates stay labelled;
/// - GPS presence only ever escalates 0→1 (documented privacy choice);
/// - `metadata_source` escalates to 'exif' once real EXIF values land, then
///   to the estimate's source (dominance order: exif > filename > mtime);
/// - `exif_at` is stamped so unchanged files drop out of the queue.
#[allow(clippy::too_many_arguments)]
pub fn upsert_exif(
    &self,
    photo_id: i64,
    e: &crate::metadata::ExifRecord,
    estimate: Option<&crate::metadata::DateEstimate>,
) -> AppResult<()> {
    let conn = self.lock()?;
    let estimate_has_metadata = e.has_metadata();
    let est_datetime = estimate.map(|est| est.datetime.as_str());
    let est_source = estimate.map(|est| est.source);
    conn.execute(
        "UPDATE photos SET
                 width = COALESCE(width, ?1),
                 height = COALESCE(height, ?2),
                 orientation = COALESCE(orientation, ?3),
                 camera_make = COALESCE(?4, camera_make),
                 camera_model = COALESCE(?5, camera_model),
                 lens = COALESCE(?6, lens),
                 lens_make = COALESCE(?7, lens_make),
                 software = COALESCE(?8, software),
                 focal_length = COALESCE(?9, focal_length),
                 iso = COALESCE(?10, iso),
                 aperture = COALESCE(?11, aperture),
                 shutter_speed = COALESCE(?12, shutter_speed),
                 capture_datetime = COALESCE(?13, ?14, capture_datetime),
                 capture_datetime_source = CASE
                     WHEN ?13 IS NOT NULL THEN 'exif'
                     WHEN ?14 IS NOT NULL THEN ?15
                     ELSE COALESCE(capture_datetime_source,
                                   CASE WHEN capture_datetime IS NOT NULL THEN 'exif' ELSE NULL END)
                 END,
                 gps_present = CASE WHEN gps_present = 1 THEN 1 ELSE ?16 END,
                 metadata_source = CASE
                     WHEN ?17 = 1 THEN 'exif'
                     WHEN ?18 IS NOT NULL THEN ?19
                     ELSE metadata_source
                 END,
                 exif_at = ?20
             WHERE id = ?21",
            params![
                e.width.map(|v| v as i64),
                e.height.map(|v| v as i64),
                e.orientation,
                e.camera_make,
                e.camera_model,
                e.lens,
                e.lens_make,
                e.software,
                e.focal_length,
                e.iso,
                e.aperture,
                e.shutter_speed,
                e.capture_datetime,
                est_datetime,
                est_source,
                i64::from(e.gps_present),
                i64::from(estimate_has_metadata),
                est_source,
                est_source,
                crate::time::now_utc(),
                photo_id,
            ],
        )
        .map_err(db_err("upsert_exif"))?;
    Ok(())
}

    /// Count + one page of photos matching a pre-built `WHERE` clause.
    /// `where_sql` must use only positionless `?` placeholders in order, and
    /// `where_params` must line up with them; `LIMIT ?` / `OFFSET ?` are
    /// appended (and parametered) by this method. Empty `where_sql` matches
    /// everything. The shared `LEFT JOIN analysis` gives filters access to
    /// the analysis columns under alias `a`.
    pub fn photos_where(
        &self,
        where_sql: &str,
        where_params: Vec<crate::filters::SqlParam>,
        offset: i64,
        limit: i64,
    ) -> AppResult<(Vec<PhotoSummary>, i64)> {
        let conn = self.lock()?;
        let limit = limit.clamp(1, 500);
        let offset = offset.max(0);
        let base = "FROM photos p LEFT JOIN analysis a ON a.photo_id = p.id";

        // Total for the caller (drives pagination + result count).
        let count_sql = format!("SELECT COUNT(*) {base} {where_sql}");
        let total: i64 = conn
            .query_row(
                count_sql.as_str(),
                rusqlite::params_from_iter(where_params.iter().map(|p| p as &dyn rusqlite::ToSql)),
                |r| r.get(0),
            )
            .map_err(db_err("count filtered photos"))?;

        let page_params: Vec<&dyn rusqlite::ToSql> = where_params
            .iter()
            .map(|p| p as &dyn rusqlite::ToSql)
            .chain([&limit as &dyn rusqlite::ToSql, &offset as &dyn rusqlite::ToSql])
            .collect();
        let page_sql = format!(
            "SELECT p.id, p.filename, p.extension, p.size_bytes, p.width, p.height,
                    p.orientation, p.capture_datetime, p.session_id,
                    (a.photo_id IS NOT NULL) AS has_analysis,
                    p.rating, p.flag, p.color_label
             {base} {where_sql}
             ORDER BY (p.capture_datetime IS NULL) ASC, p.capture_datetime ASC, p.id ASC
             LIMIT ? OFFSET ?"
        );
        let mut stmt = conn.prepare(page_sql.as_str()).map_err(db_err("prepare filtered photos"))?;
        let rows = stmt
            .query_map(page_params.as_slice(), |r| {
                Ok(PhotoSummary {
                    id: r.get(0)?,
                    filename: r.get(1)?,
                    extension: r.get(2)?,
                    size_bytes: r.get(3)?,
                    width: r.get(4)?,
                    height: r.get(5)?,
                    orientation: r.get(6)?,
                    capture_datetime: r.get(7)?,
                    session_id: r.get(8)?,
                    has_analysis: r.get::<_, i64>(9)? != 0,
                    rating: r.get(10)?,
                    flag: r.get::<_, i64>(11)? != 0,
                    color_label: r.get(12)?,
                })
            })
            .map_err(db_err("query filtered photos"))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(db_err("read filtered photo row"))?);
        }
        Ok((out, total))
    }

    /// `list_photos` is the unfiltered page (kept as the default grid path).
    pub fn list_photos(&self, offset: i64, limit: i64) -> AppResult<(Vec<PhotoSummary>, i64)> {
        self.photos_where("", Vec::new(), offset, limit)
    }

    /// Distinct metadata values for select-style filters. The column name is
    /// a fixed allowlist, never caller-provided SQL; blank EXIF values count
    /// as unidentified alongside NULL values.
    pub fn filter_value_options(
        &self,
        field: &str,
        session_id: Option<i64>,
    ) -> AppResult<FilterValueOptions> {
        let column = match field {
            "camera_make" => "camera_make",
            "camera_model" => "camera_model",
            "lens" => "lens",
            _ => return Err(AppError::validation(format!("`{field}` has no selectable metadata values"))),
        };
        let conn = self.lock()?;
        let scope = session_id.map(|_| " AND session_id = ?1").unwrap_or("");
        let values_sql = format!(
            "SELECT {column}, COUNT(*) FROM photos
             WHERE {column} IS NOT NULL AND TRIM({column}) <> ''{scope}
             GROUP BY {column} ORDER BY COUNT(*) DESC, {column} COLLATE NOCASE ASC"
        );
        let mut values_stmt = conn
            .prepare(&values_sql)
            .map_err(db_err("prepare metadata filter values"))?;
        let values = if let Some(id) = session_id {
            values_stmt
                .query_map(params![id], |row| {
                    Ok(FilterValueOption { value: row.get(0)?, count: row.get(1)? })
                })
                .map_err(db_err("query metadata filter values"))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_err("read metadata filter values"))?
        } else {
            values_stmt
                .query_map([], |row| {
                    Ok(FilterValueOption { value: row.get(0)?, count: row.get(1)? })
                })
                .map_err(db_err("query metadata filter values"))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_err("read metadata filter values"))?
        };

        let unknown_sql = format!(
            "SELECT COUNT(*) FROM photos
             WHERE ({column} IS NULL OR TRIM({column}) = ''){scope}"
        );
        let unidentified_count = if let Some(id) = session_id {
            conn.query_row(&unknown_sql, params![id], |row| row.get(0))
        } else {
            conn.query_row(&unknown_sql, [], |row| row.get(0))
        }
        .map_err(db_err("count unidentified metadata values"))?;

        Ok(FilterValueOptions { values, unidentified_count })
    }

    /// Availability and observed bounds for numeric quick-filter controls.
    /// Expressions come from this allowlist only; the caller never supplies
    /// SQL. Missing values remain a first-class count and are never inferred.
    pub fn numeric_filter_stats(
        &self,
        field: &str,
        session_id: Option<i64>,
    ) -> AppResult<NumericFilterStats> {
        let expr = match field {
            "sharpness" => "a.sharpness",
            "brightness" => "a.brightness",
            "contrast" => "a.contrast",
            "iso" => "p.iso",
            "focal_length" => "p.focal_length",
            _ => {
                return Err(AppError::validation(format!(
                    "`{field}` has no numeric filter statistics"
                )))
            }
        };
        let sql = format!(
            "SELECT COUNT({expr}), COUNT(*) - COUNT({expr}), MIN({expr}), MAX({expr})
               FROM photos p
               LEFT JOIN analysis a ON a.photo_id = p.id
              WHERE (?1 IS NULL OR p.session_id = ?1)"
        );
        let conn = self.lock()?;
        conn.query_row(&sql, params![session_id], |row| {
            Ok(NumericFilterStats {
                recorded_count: row.get(0)?,
                missing_count: row.get(1)?,
                minimum: row.get(2)?,
                maximum: row.get(3)?,
            })
        })
        .map_err(db_err("read numeric filter statistics"))
    }

    /// Session-first review data. This returns lightweight grid rows plus the
    /// existing similarity/burst memberships in one local SQLite transaction;
    /// pixel decoding remains on demand in the UI.
    pub fn review_queue(&self, session_id: i64) -> AppResult<ReviewQueue> {
        let conn = self.lock()?;
        let mut photo_stmt = conn
            .prepare(
                "SELECT p.id, p.filename, p.extension, p.size_bytes, p.width, p.height,
                        p.orientation, p.capture_datetime, p.session_id,
                        (a.photo_id IS NOT NULL) AS has_analysis,
                        p.rating, p.flag, p.color_label
                 FROM photos p LEFT JOIN analysis a ON a.photo_id = p.id
                 WHERE p.session_id = ?1
                 ORDER BY (p.capture_datetime IS NULL) ASC, p.capture_datetime ASC, p.id ASC",
            )
            .map_err(db_err("prepare review photos"))?;
        let photo_rows = photo_stmt
            .query_map([session_id], |r| {
                Ok(PhotoSummary {
                    id: r.get(0)?,
                    filename: r.get(1)?,
                    extension: r.get(2)?,
                    size_bytes: r.get(3)?,
                    width: r.get(4)?,
                    height: r.get(5)?,
                    orientation: r.get(6)?,
                    capture_datetime: r.get(7)?,
                    session_id: r.get(8)?,
                    has_analysis: r.get::<_, i64>(9)? != 0,
                    rating: r.get(10)?,
                    flag: r.get::<_, i64>(11)? != 0,
                    color_label: r.get(12)?,
                })
            })
            .map_err(db_err("query review photos"))?;
        let mut photos = Vec::new();
        for row in photo_rows {
            photos.push(row.map_err(db_err("read review photo"))?);
        }

        let mut group_stmt = conn
            .prepare(
                "SELECT g.id, g.group_type, gp.photo_id
                 FROM similarity_groups g
                 JOIN similarity_group_photos gp ON gp.group_id = g.id
                 JOIN photos p ON p.id = gp.photo_id
                 WHERE p.session_id = ?1
                 ORDER BY CASE g.group_type WHEN 'burst' THEN 0 ELSE 1 END,
                          g.id,
                          (p.capture_datetime IS NULL) ASC, p.capture_datetime ASC, p.id ASC",
            )
            .map_err(db_err("prepare review sequences"))?;
        let group_rows = group_stmt
            .query_map([session_id], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))
            })
            .map_err(db_err("query review sequences"))?;
        let mut sequences: Vec<ReviewSequence> = Vec::new();
        let mut sequence_indices: HashMap<(i64, String), usize> = HashMap::new();
        for row in group_rows {
            let (id, group_type, photo_id) = row.map_err(db_err("read review sequence"))?;
            let key = (id, group_type.clone());
            if let Some(index) = sequence_indices.get(&key) {
                sequences[*index].photo_ids.push(photo_id);
            } else {
                let index = sequences.len();
                sequence_indices.insert(key, index);
                sequences.push(ReviewSequence {
                    id,
                    group_type,
                    photo_ids: vec![photo_id],
                });
            }
        }
        sequences.retain(|sequence| sequence.photo_ids.len() >= 2);
        Ok(ReviewQueue { photos, sequences })
    }

    pub fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, AppError> {
        self.conn
            .lock()
            .map_err(|e| AppError::Database(format!("database lock poisoned: {e}")))
    }
/// Update a photo's location after a successful rename/move. The pixels are
/// unchanged, so analysis rows stay valid; `file_mtime` is refreshed so the
/// incremental analysis rule re-checks if the FS changed anything.
pub fn update_photo_path(
    &self,
    id: i64,
    path: &str,
    filename: &str,
    size_bytes: Option<i64>,
    file_mtime: Option<&str>,
) -> AppResult<()> {
    let conn = self.lock()?;
    conn.execute(
        "UPDATE photos
         SET path = ?2, filename = ?3,
             size_bytes = COALESCE(?4, size_bytes),
             file_mtime = COALESCE(?5, file_mtime)
         WHERE id = ?1",
        params![id, path, filename, size_bytes, file_mtime],
    )
    .map_err(db_err("update photo path"))?;
    Ok(())
}
/// Remove photo rows (and, via FK cascade, their analysis + selection rows)
/// after a successful trash. The filesystem action already happened.
pub fn delete_photos(&self, ids: Vec<i64>) -> AppResult<usize> {
    if ids.is_empty() {
        return Ok(0);
    }
    let conn = self.lock()?;
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let params: Vec<Box<dyn rusqlite::ToSql>> =
        ids.iter().map(|id| Box::new(*id) as Box<dyn rusqlite::ToSql>).collect();
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let n = conn
        .execute(&format!("DELETE FROM photos WHERE id IN ({placeholders})"), refs.as_slice())
        .map_err(db_err("delete photos"))?;
    Ok(n)
}
/// Append one executed (or attempted) operation. The audit log is the only
/// permanent record of what happened to files.
pub fn record_file_op(
    &self,
    op_type: &str,
    source_path: &str,
    dest_path: Option<&str>,
    status: &str,
    detail: Option<&str>,
) -> AppResult<()> {
    let conn = self.lock()?;
    conn.execute(
        "INSERT INTO file_operations (op_type, source_path, dest_path, status, detail, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![op_type, source_path, dest_path, status, detail, crate::time::now_utc()],
    )
    .map_err(db_err("record file op"))?;
    Ok(())
}
pub fn recent_file_ops(&self, limit: i64) -> AppResult<Vec<FileOpRow>> {
    let conn = self.lock()?;
    let mut stmt = conn
        .prepare(
            "SELECT id, op_type, source_path, dest_path, status, detail, created_at
             FROM file_operations ORDER BY id DESC LIMIT ?1",
        )
        .map_err(db_err("prepare file ops"))?;
    let rows = stmt
        .query_map([limit.max(1).min(500)], |r| {
            Ok(FileOpRow {
                id: r.get(0)?,
                op_type: r.get(1)?,
                source_path: r.get(2)?,
                dest_path: r.get(3)?,
                status: r.get(4)?,
                detail: r.get(5)?,
                created_at: r.get(6)?,
            })
        })
        .map_err(db_err("query file ops"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(db_err("read file op row"))?);
    }
    Ok(out)
}

    /// Validate a selection state against the closed set (`selected`/`rejected`).
    pub fn validate_selection_state(state: &str) -> AppResult<&'static str> {
        SELECTION_STATES
            .iter()
            .find(|s| **s == state)
            .copied()
            .ok_or_else(|| AppError::validation(format!("Unknown selection state: {state}")))
    }
pub fn set_selection(&self, photo_id: i64, state: &str) -> AppResult<()> {
    let state = Self::validate_selection_state(state)?;
    let conn = self.lock()?;
    conn.execute(
        "INSERT INTO selections (photo_id, state, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(photo_id) DO UPDATE SET state = excluded.state,
                                                   updated_at = excluded.updated_at",
        params![photo_id, state, crate::time::now_utc()],
    )
    .map_err(db_err("set selection"))?;
    Ok(())
}
/// Batch variant (the UI applies culling in groups).
pub fn set_selections(&self, photo_ids: Vec<i64>, state: &str) -> AppResult<usize> {
    let state = Self::validate_selection_state(state)?;
    if photo_ids.is_empty() {
        return Ok(0);
    }
    let conn = self.lock()?;
    let now = crate::time::now_utc();
    let tx = conn.unchecked_transaction().map_err(db_err("selection tx"))?;
    let mut n = 0usize;
    for id in &photo_ids {
        n += tx.execute(
            "INSERT INTO selections (photo_id, state, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(photo_id) DO UPDATE SET state = excluded.state,
                                                       updated_at = excluded.updated_at",
            params![id, state, now],
        )
        .map_err(db_err("batch set selection"))?;
    }
    tx.commit().map_err(db_err("commit selection tx"))?;
    Ok(n)
}
pub fn clear_selection(&self, photo_id: i64) -> AppResult<()> {
    let conn = self.lock()?;
    conn.execute(
        "DELETE FROM selections WHERE photo_id = ?1",
        [photo_id],
    )
    .map_err(db_err("clear selection"))?;
    Ok(())
}

/// The fixed set of color labels the UI offers (Sprint 13).
pub const COLOR_LABELS: [&str; 6] = ["red", "yellow", "green", "blue", "purple", "gray"];

/// Apply curatorial marks (Sprint 13) to a batch of photos. Only the fields
/// supplied as `Some` change; `None` leaves that mark untouched. Clearing:
/// `rating: Some(0)` un-rates (back to NULL), `color: Some("")` removes the
/// label; `flag: Some(false)` un-flags. Values are validated against the
/// closed ranges/enum before any SQL runs.
pub fn set_marks(
    &self,
    photo_ids: &[i64],
    rating: Option<i64>,
    flag: Option<bool>,
    color: Option<&str>,
) -> AppResult<usize> {
    if photo_ids.is_empty() {
        return Ok(0);
    }
    if let Some(r) = rating {
        if !(0..=5).contains(&r) {
            return Err(AppError::validation(format!(
                "Rating must be between 0 and 5 stars (0 clears the rating), got {r}"
            )));
        }
    }
    let color_ok = color
        .map(|c| c.is_empty() || Self::COLOR_LABELS.contains(&c))
        .unwrap_or(true);
    if !color_ok {
        return Err(AppError::validation(format!(
            "Unknown color label: {:?}. Use one of {:?} (or an empty string to clear)",
            color,
            Self::COLOR_LABELS.as_slice()
        )));
    }

    let conn = self.lock()?;
    let placeholders = photo_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let mut sets: Vec<&str> = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    match rating {
        None => {}
        Some(0) => {
            sets.push("rating = NULL");
        }
        Some(r) => {
            sets.push("rating = ?");
            params.push(Box::new(r));
        }
    }
    match flag {
        None => {}
        Some(f) => {
            sets.push("flag = ?");
            params.push(Box::new(i64::from(f)));
        }
    }
    match color {
        None => {}
        Some("") => {
            sets.push("color_label = NULL");
        }
        Some(c) => {
            sets.push("color_label = ?");
            params.push(Box::new(c.to_string()));
        }
    }
    if sets.is_empty() {
        return Ok(0);
    }
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let sql = format!(
        "UPDATE photos SET {} WHERE id IN ({})",
        sets.join(", "),
        placeholders
    );
    let all_params: Vec<&dyn rusqlite::ToSql> = refs
        .iter()
        .copied()
        .chain(photo_ids.iter().map(|id| id as &dyn rusqlite::ToSql))
        .collect();
    let n = conn
        .execute(sql.as_str(), all_params.as_slice())
        .map_err(db_err("set marks"))?;
    Ok(n)
}
    pub fn clear_selections(&self, photo_ids: Vec<i64>) -> AppResult<usize> {
    if photo_ids.is_empty() {
        return Ok(0);
    }
    let conn = self.lock()?;
    let placeholders = photo_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let params: Vec<Box<dyn rusqlite::ToSql>> = photo_ids
        .iter()
        .map(|id| Box::new(*id) as Box<dyn rusqlite::ToSql>)
        .collect();
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let n = conn
        .execute(
            &format!("DELETE FROM selections WHERE photo_id IN ({placeholders})"),
            refs.as_slice(),
        )
        .map_err(db_err("clear selections"))?;
    Ok(n)
}
/// Current culling state (photo id + state), most recent first. Capped so a
/// pathological library can never ship an unbounded payload.
pub fn list_selections(&self, limit: i64) -> AppResult<Vec<SelectionRow>> {
    let conn = self.lock()?;
    let mut stmt = conn
        .prepare(
            "SELECT photo_id, state, updated_at FROM selections
             ORDER BY updated_at DESC, photo_id LIMIT ?1",
        )
        .map_err(db_err("prepare selections"))?;
    let rows = stmt
        .query_map([limit.max(1).min(20_000)], |r| {
            Ok(SelectionRow {
                photo_id: r.get(0)?,
                state: r.get(1)?,
                updated_at: r.get(2)?,
            })
        })
        .map_err(db_err("query selections"))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(db_err("read selection row"))?);
    }
    Ok(out)
}

    // -----------------------------------------------------------------
    // Saved views (Sprint 8) — a saved view is a name + a structured filter.
    // -----------------------------------------------------------------

    pub fn list_saved_views(&self) -> AppResult<Vec<SavedView>> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, filter_json, description, created_at, updated_at
                 FROM saved_views ORDER BY name COLLATE NOCASE",
            )
            .map_err(db_err("prepare saved views"))?;
        let rows = stmt
            .query_map([], |r| {
                Ok(SavedView {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    filter_json: r.get(2)?,
                    description: r.get(3)?,
                    created_at: r.get(4)?,
                    updated_at: r.get(5)?,
                })
            })
            .map_err(db_err("query saved views"))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(db_err("read saved view row"))?);
        }
        Ok(out)
    }

    /// Create or overwrite a saved view by name (UNIQUE). The filter must be
    /// a valid structured filter (validated by the filter engine before this
    /// is called).
    pub fn upsert_saved_view(
        &self,
        name: &str,
        filter_json: &str,
        description: Option<&str>,
    ) -> AppResult<i64> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::validation("View name is empty".to_string()));
        }
        let conn = self.lock()?;
        let now = crate::time::now_utc();
        conn.execute(
            "INSERT INTO saved_views (name, filter_json, description, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(name) DO UPDATE SET
               filter_json  = excluded.filter_json,
               description  = COALESCE(excluded.description, saved_views.description),
               updated_at   = excluded.updated_at",
            params![name, filter_json, description, now, now],
        )
        .map_err(db_err("upsert saved view"))?;
        let id: i64 = conn
            .query_row("SELECT id FROM saved_views WHERE name = ?1", [name], |r| r.get(0))
            .map_err(db_err("saved view id lookup"))?;
        Ok(id)
    }

    pub fn rename_saved_view(&self, id: i64, name: &str) -> AppResult<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::validation("View name is empty".to_string()));
        }
        let conn = self.lock()?;
        let now = crate::time::now_utc();
        let n = conn
            .execute(
                "UPDATE saved_views SET name = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, name, now],
            )
            .map_err(db_err("rename saved view"))?;
        if n == 0 {
            return Err(AppError::FileMissing {
                target: format!("saved view {id}"),
                reason: "not found".into(),
            });
        }
        Ok(())
    }

    pub fn delete_saved_view(&self, id: i64) -> AppResult<()> {
        let conn = self.lock()?;
        conn.execute("DELETE FROM saved_views WHERE id = ?1", [id])
            .map_err(db_err("delete saved view"))?;
        Ok(())
    }

    // -----------------------------------------------------------------
    // Collections (Sprint 8) — manually curated sets of photographs.
    // -----------------------------------------------------------------

    pub fn list_collections(&self) -> AppResult<Vec<Collection>> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT c.id, c.name, c.description, c.created_at,
                        (SELECT COUNT(*) FROM collection_photos cp WHERE cp.collection_id = c.id)
                 FROM collections c ORDER BY c.created_at",
            )
            .map_err(db_err("prepare collections"))?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Collection {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    description: r.get(2)?,
                    created_at: r.get(3)?,
                    photo_count: r.get(4)?,
                })
            })
            .map_err(db_err("query collections"))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(db_err("read collection row"))?);
        }
        Ok(out)
    }

    pub fn create_collection(&self, name: &str, description: Option<&str>) -> AppResult<i64> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::validation("Collection name is empty".to_string()));
        }
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO collections (name, description, created_at) VALUES (?1, ?2, ?3)",
            params![name, description, crate::time::now_utc()],
        )
        .map_err(|e| {
            if e.to_string().contains("UNIQUE") {
                AppError::validation(format!("A collection named “{name}” already exists"))
            } else {
                db_err("create collection")(e)
            }
        })?;
        let id: i64 = conn
            .query_row("SELECT id FROM collections WHERE name = ?1", [name], |r| r.get(0))
            .map_err(db_err("collection id lookup"))?;
        Ok(id)
    }

    pub fn rename_collection(&self, id: i64, name: &str) -> AppResult<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::validation("Collection name is empty".to_string()));
        }
        let conn = self.lock()?;
        let n = conn
            .execute("UPDATE collections SET name = ?2 WHERE id = ?1", params![id, name])
            .map_err(|e| {
                if e.to_string().contains("UNIQUE") {
                    AppError::validation(format!("A collection named “{name}” already exists"))
                } else {
                    db_err("rename collection")(e)
                }
            })?;
        if n == 0 {
            return Err(AppError::FileMissing {
                target: format!("collection {id}"),
                reason: "not found".into(),
            });
        }
        Ok(())
    }

    pub fn delete_collection(&self, id: i64) -> AppResult<()> {
        let conn = self.lock()?;
        // `collection_photos` cascade-deletes with the collection; the photos
        // themselves are untouched.
        conn.execute("DELETE FROM collections WHERE id = ?1", [id])
            .map_err(db_err("delete collection"))?;
        Ok(())
    }

    /// Add photographs to a collection (idempotent — duplicates are ignored).
    /// Returns how many were actually added.
    pub fn add_to_collection(&self, collection_id: i64, photo_ids: Vec<i64>) -> AppResult<usize> {
        let conn = self.lock()?;
        if !Self::collection_exists(&conn, collection_id) {
            return Err(AppError::FileMissing {
                target: format!("collection {collection_id}"),
                reason: "not found".into(),
            });
        }
        let now = crate::time::now_utc();
        let mut added = 0usize;
        let tx = conn.unchecked_transaction().map_err(db_err("collection add tx"))?;
        for id in photo_ids {
            added += tx.execute(
                "INSERT OR IGNORE INTO collection_photos (collection_id, photo_id, added_at)
                 VALUES (?1, ?2, ?3)",
                params![collection_id, id, now],
            )
            .map_err(db_err("add to collection"))?;
        }
        tx.commit().map_err(db_err("commit collection add"))?;
        Ok(added)
    }

    pub fn remove_from_collection(&self, collection_id: i64, photo_ids: Vec<i64>) -> AppResult<usize> {
        if photo_ids.is_empty() {
            return Ok(0);
        }
        let conn = self.lock()?;
        let placeholders = photo_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let params: Vec<Box<dyn rusqlite::ToSql>> = photo_ids
            .iter()
            .map(|id| Box::new(*id) as Box<dyn rusqlite::ToSql>)
            .collect();
        let mut refs: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(collection_id)];
        refs.extend(params);
        let ref_refs: Vec<&dyn rusqlite::ToSql> = refs.iter().map(|p| p.as_ref()).collect();
        let n = conn
            .execute(
                &format!(
                    "DELETE FROM collection_photos WHERE collection_id = ?1 AND photo_id IN ({placeholders})"
                ),
                ref_refs.as_slice(),
            )
            .map_err(db_err("remove from collection"))?;
        Ok(n)
    }

    /// One page of a collection's photographs (same summary shape as the grid).
    pub fn collection_photos(
        &self,
        collection_id: i64,
        offset: i64,
        limit: i64,
    ) -> AppResult<(Vec<PhotoSummary>, i64)> {
        let conn = self.lock()?;
        let limit = limit.clamp(1, 500);
        let offset = offset.max(0);
        if !Self::collection_exists(&conn, collection_id) {
            return Err(AppError::FileMissing {
                target: format!("collection {collection_id}"),
                reason: "not found".into(),
            });
        }
        let total: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM collection_photos WHERE collection_id = ?1",
                [collection_id],
                |r| r.get(0),
            )
            .map_err(db_err("count collection photos"))?;
        let mut stmt = conn
            .prepare(
                "SELECT p.id, p.filename, p.extension, p.size_bytes, p.width, p.height,
                        p.orientation, p.capture_datetime, p.session_id,
                        (a.photo_id IS NOT NULL) AS has_analysis,
                        p.rating, p.flag, p.color_label
                 FROM collection_photos cp
                 JOIN photos p ON p.id = cp.photo_id
                 LEFT JOIN analysis a ON a.photo_id = p.id
                 WHERE cp.collection_id = ?1
                 ORDER BY cp.added_at, cp.photo_id
                 LIMIT ?2 OFFSET ?3",
            )
            .map_err(db_err("prepare collection photos"))?;
        let rows = stmt
            .query_map(params![collection_id, limit, offset], Self::page_row_to_summary)
            .map_err(db_err("query collection photos"))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(db_err("read collection photo row"))?);
        }
        Ok((out, total))
    }

    /// Which of these photos are in the collection (for the grid's badges).
    pub fn collection_ids_for_photos(&self, photo_ids: Vec<i64>) -> AppResult<Vec<i64>> {
        if photo_ids.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.lock()?;
        let placeholders = photo_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let params: Vec<Box<dyn rusqlite::ToSql>> = photo_ids
            .iter()
            .map(|id| Box::new(*id) as Box<dyn rusqlite::ToSql>)
            .collect();
        let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let mut stmt = conn
            .prepare(&format!(
                "SELECT DISTINCT collection_id FROM collection_photos WHERE photo_id IN ({placeholders})"
            ))
            .map_err(db_err("prepare collection ids"))?;
        let rows = stmt
            .query_map(refs.as_slice(), |r| r.get::<_, i64>(0))
            .map_err(db_err("query collection ids"))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(db_err("read collection id"))?);
        }
        Ok(out)
    }

    fn collection_exists(conn: &Connection, collection_id: i64) -> bool {
        conn.query_row(
            "SELECT 1 FROM collections WHERE id = ?1",
            [collection_id],
            |_| Ok(()),
        )
        .optional()
        .ok()
        .flatten()
        .is_some()
    }

    fn page_row_to_summary(r: &rusqlite::Row) -> rusqlite::Result<PhotoSummary> {
        Ok(PhotoSummary {
            id: r.get(0)?,
            filename: r.get(1)?,
            extension: r.get(2)?,
            size_bytes: r.get(3)?,
            width: r.get(4)?,
            height: r.get(5)?,
            orientation: r.get(6)?,
            capture_datetime: r.get(7)?,
            session_id: r.get(8)?,
            has_analysis: r.get::<_, i64>(9)? != 0,
            rating: r.get(10)?,
            flag: r.get::<_, i64>(11)? != 0,
            color_label: r.get(12)?,
        })
    }

    // -----------------------------------------------------------------
    // Similarity groups (Sprint 8) — perceptual-hash clusters + bursts.
    // -----------------------------------------------------------------

    pub fn list_similarity_groups(&self, limit: i64) -> AppResult<Vec<SimilarityGroup>> {
        let conn = self.lock()?;
        let limit = limit.clamp(1, 500);
        let Some(session_id) = self.active_session_id(&conn)? else {
            return Ok(Vec::new());
        };
        let mut stmt = conn
            .prepare(
                "SELECT g.id, g.hash, g.group_type, g.photo_count, g.created_at,
                        (SELECT COUNT(DISTINCT p.session_id)
                         FROM similarity_group_photos gp
                         JOIN photos p ON p.id = gp.photo_id
                         WHERE gp.group_id = g.id) AS session_count
                 FROM similarity_groups g
                 WHERE g.session_id = ?1
                 ORDER BY (g.group_type = 'burst') DESC, g.photo_count DESC, g.id
                 LIMIT ?2",
            )
            .map_err(db_err("prepare similarity groups"))?;
        let rows = stmt
            .query_map(params![session_id, limit], |r| {
                Ok(SimilarityGroup {
                    id: r.get(0)?,
                    hash: r.get(1)?,
                    group_type: r.get(2)?,
                    photo_count: r.get(3)?,
                    created_at: r.get(4)?,
                    session_count: r.get(5)?,
                    cover_photos: Vec::new(),
                })
            })
            .map_err(db_err("query similarity groups"))?;
        let mut groups: Vec<SimilarityGroup> = Vec::new();
        for row in rows {
            groups.push(row.map_err(db_err("read similarity group row"))?);
        }
        // Second pass: a small cover strip per group (bounded, keeps this one
        // round-trip per group with a tight LIMIT).
        let mut covers = conn
            .prepare("SELECT photo_id FROM similarity_group_photos WHERE group_id = ?1 ORDER BY photo_id LIMIT 4")
            .map_err(db_err("prepare group covers"))?;
        for g in groups.iter_mut() {
            let ids = covers
                .query_map([g.id], |r| r.get::<_, i64>(0))
                .map_err(db_err("query group covers"))?
                .collect::<Result<_, _>>()
                .map_err(db_err("read group covers"))?;
            g.cover_photos = ids;
        }
        Ok(groups)
    }

    pub fn group_photos(
        &self,
        group_id: i64,
        offset: i64,
        limit: i64,
    ) -> AppResult<(Vec<PhotoSummary>, i64)> {
        let conn = self.lock()?;
        let limit = limit.clamp(1, 500);
        let offset = offset.max(0);
        let Some(session_id) = self.active_session_id(&conn)? else {
            return Ok((Vec::new(), 0));
        };
        let total: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM similarity_group_photos gp
                 JOIN similarity_groups g ON g.id = gp.group_id
                 WHERE gp.group_id = ?1 AND g.session_id = ?2",
                params![group_id, session_id],
                |r| r.get(0),
            )
            .map_err(db_err("count group photos"))?;
        if total == 0 {
            return Ok((Vec::new(), 0));
        }
        let mut stmt = conn
            .prepare(
                "SELECT p.id, p.filename, p.extension, p.size_bytes, p.width, p.height,
                        p.orientation, p.capture_datetime, p.session_id,
                        (a.photo_id IS NOT NULL) AS has_analysis,
                        p.rating, p.flag, p.color_label
                 FROM similarity_group_photos g
                 JOIN photos p ON p.id = g.photo_id
                 LEFT JOIN analysis a ON a.photo_id = p.id
                 JOIN similarity_groups sg ON sg.id = g.group_id
                 WHERE g.group_id = ?1 AND sg.session_id = ?2
                 ORDER BY (p.capture_datetime IS NULL) ASC, p.capture_datetime ASC, p.id ASC
                 LIMIT ?3 OFFSET ?4",
            )
            .map_err(db_err("prepare group photos"))?;
        let rows = stmt
            .query_map(params![group_id, session_id, limit, offset], Self::page_row_to_summary)
            .map_err(db_err("query group photos"))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(db_err("read group photo row"))?);
        }
        Ok((out, total))
    }

    /// Replace one project's similarity-group set atomically. Groups for
    /// every other project remain available when that project is reopened.
    pub fn replace_similarity_groups_for_session(
        &self,
        session_id: i64,
        groups: &[(String, String, Vec<i64>)],
    ) -> AppResult<usize> {
        let conn = self.lock()?;
        let now = crate::time::now_utc();
        let tx = conn.unchecked_transaction().map_err(db_err("similarity tx"))?;
        tx.execute(
            "DELETE FROM similarity_group_photos
             WHERE group_id IN (SELECT id FROM similarity_groups WHERE session_id = ?1)",
            [session_id],
        )
        .map_err(db_err("clear project group photos"))?;
        tx.execute("DELETE FROM similarity_groups WHERE session_id = ?1", [session_id])
            .map_err(db_err("clear project groups"))?;
        for (hash, group_type, photo_ids) in groups {
            if photo_ids.is_empty() {
                continue;
            }
            tx.execute(
                "INSERT INTO similarity_groups (hash, group_type, photo_count, created_at, session_id)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![hash, group_type, photo_ids.len() as i64, now, session_id],
            )
            .map_err(db_err("insert group"))?;
            let gid: i64 =
                tx.query_row("SELECT last_insert_rowid()", [], |r| r.get(0))
                    .map_err(db_err("group id"))?;
            for pid in photo_ids {
                tx.execute(
                    "INSERT OR IGNORE INTO similarity_group_photos (group_id, photo_id)
                     VALUES (?1, ?2)",
                    params![gid, pid],
                )
                .map_err(db_err("insert group photo"))?;
            }
        }
        tx.commit().map_err(db_err("commit similarity"))?;
        Ok(groups.len())
    }

    /// All currently hashed photos for one project/session. The similarity
    /// pass must never read another project's cached rows.
    pub fn hashed_photos_for_session(
        &self,
        session_id: i64,
    ) -> AppResult<Vec<(i64, i64, Option<i64>, Option<String>)>> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare("SELECT id, phash, session_id, capture_datetime
                      FROM photos WHERE phash IS NOT NULL AND session_id = ?1")
            .map_err(db_err("prepare hashed photos"))?;
        let rows = stmt
            .query_map([session_id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, Option<i64>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(db_err("query hashed photos"))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(db_err("read hashed photo"))?);
        }
        Ok(out)
    }

    // --- Perceptual-hash queue + storage (v9) ---

    /// Photos still lacking a phash, or hashed from a now-stale mtime (same
    /// incremental rule as analysis). Decodable formats only.
    pub fn phash_queue(&self) -> AppResult<Vec<PhashWork>> {
        let conn = self.lock()?;
        let active_sid = self.active_session_id(&conn)?;
        let mut out = Vec::new();
        if let Some(sid) = active_sid {
            let mut stmt = conn
                .prepare(
                    "SELECT id, path, extension, file_mtime FROM photos
                     WHERE extension IN ('jpg', 'jpeg', 'png', 'webp', 'tif', 'tiff')
                       AND (phash IS NULL
                            OR (phash_source_mtime IS NOT NULL AND file_mtime IS NOT NULL
                                AND phash_source_mtime < file_mtime))
                       AND session_id = ?1
                     ORDER BY (capture_datetime IS NULL) ASC, COALESCE(capture_datetime, indexed_at), id",
                )
                .map_err(db_err("prepare phash queue"))?;
            for row in stmt.query_map(params![sid], |r| {
                Ok(PhashWork {
                    photo_id: r.get(0)?,
                    path: r.get(1)?,
                    extension: r.get(2)?,
                    file_mtime: r.get(3)?,
                })
            }).map_err(db_err("query phash queue"))? {
                out.push(row.map_err(db_err("read phash work row"))?);
            }
        } else {
            let mut stmt = conn
                .prepare(
                    "SELECT id, path, extension, file_mtime FROM photos
                     WHERE extension IN ('jpg', 'jpeg', 'png', 'webp', 'tif', 'tiff')
                       AND (phash IS NULL
                            OR (phash_source_mtime IS NOT NULL AND file_mtime IS NOT NULL
                                AND phash_source_mtime < file_mtime))
                     ORDER BY (capture_datetime IS NULL) ASC, COALESCE(capture_datetime, indexed_at), id",
                )
                .map_err(db_err("prepare phash queue"))?;
            for row in stmt.query_map([], |r| {
                Ok(PhashWork {
                    photo_id: r.get(0)?,
                    path: r.get(1)?,
                    extension: r.get(2)?,
                    file_mtime: r.get(3)?,
                })
            }).map_err(db_err("query phash queue"))? {
                out.push(row.map_err(db_err("read phash work row"))?);
            }
        }
        Ok(out)
    }

    pub fn upsert_phash(
        &self,
        photo_id: i64,
        hash: i64,
        source_mtime: Option<&str>,
    ) -> AppResult<()> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE photos SET phash = ?2, phash_source_mtime = ?3 WHERE id = ?1",
            params![photo_id, hash, source_mtime],
        )
        .map_err(db_err("upsert phash"))?;
        Ok(())
    }

    // --- Face-detection queue + storage (v10, Sprint 9 local AI) ---

    /// Photos still lacking a face result, or detected from a now-stale
    /// mtime (same incremental rule as the phash pass). Decodable formats
    /// only. Carries the stored dimensions so the pass can apply its pixel
    /// guard before decoding.
    pub fn faces_queue(&self) -> AppResult<Vec<FaceWork>> {
        let conn = self.lock()?;
        let active_sid = self.active_session_id(&conn)?;
        let mut out = Vec::new();
        if let Some(sid) = active_sid {
            let mut stmt = conn
                .prepare(
                    "SELECT p.id, p.path, p.width, p.height, p.file_mtime
                     FROM photos p
                     LEFT JOIN analysis a ON a.photo_id = p.id
                     WHERE p.session_id = ?1
                       AND p.extension IN ('jpg', 'jpeg', 'png', 'webp', 'tif', 'tiff')
                       AND (a.face_count IS NULL
                            OR (p.file_mtime IS NOT NULL AND a.faces_at IS NOT NULL
                                AND a.faces_at < p.file_mtime))
                     ORDER BY (p.capture_datetime IS NULL) ASC,
                              COALESCE(p.capture_datetime, p.indexed_at), p.id",
                )
                .map_err(db_err("prepare faces queue"))?;
            for row in stmt.query_map(params![sid], |r| {
                    Ok(FaceWork {
                        photo_id: r.get(0)?,
                        path: r.get(1)?,
                        width: r.get(2)?,
                        height: r.get(3)?,
                        file_mtime: r.get(4)?,
                    })
                }).map_err(db_err("query faces queue"))? {
                out.push(row.map_err(db_err("read face work row"))?);
            }
        } else {
            let mut stmt = conn
                .prepare(
                    "SELECT p.id, p.path, p.width, p.height, p.file_mtime
                     FROM photos p
                     LEFT JOIN analysis a ON a.photo_id = p.id
                     WHERE p.extension IN ('jpg', 'jpeg', 'png', 'webp', 'tif', 'tiff')
                       AND (a.face_count IS NULL
                            OR (p.file_mtime IS NOT NULL AND a.faces_at IS NOT NULL
                                AND a.faces_at < p.file_mtime))
                     ORDER BY (p.capture_datetime IS NULL) ASC,
                              COALESCE(p.capture_datetime, p.indexed_at), p.id",
                )
                .map_err(db_err("prepare faces queue"))?;
            for row in stmt.query_map([], |r| {
                    Ok(FaceWork {
                        photo_id: r.get(0)?,
                        path: r.get(1)?,
                        width: r.get(2)?,
                        height: r.get(3)?,
                        file_mtime: r.get(4)?,
                    })
                }).map_err(db_err("query faces queue"))? {
                out.push(row.map_err(db_err("read face work row"))?);
            }
        }
        Ok(out)
    }

    /// Photos queued for scene classification (Sprint 18): decodable files
    /// missing a scene result, or whose file is newer than the last run —
    /// the same incremental rule as faces/EXIF/similarity.
    pub fn scenes_queue(&self) -> AppResult<Vec<SceneWork>> {
        let conn = self.lock()?;
        let active_sid = self.active_session_id(&conn)?;
        let mut out = Vec::new();
        if let Some(sid) = active_sid {
            let mut stmt = conn
                .prepare(
                    "SELECT p.id, p.path, p.file_mtime
                     FROM photos p
                     LEFT JOIN analysis a ON a.photo_id = p.id
                     WHERE p.session_id = ?1
                       AND p.extension IN ('jpg', 'jpeg', 'png', 'webp', 'tif', 'tiff')
                       AND (a.scene_fine IS NULL
                            OR (p.file_mtime IS NOT NULL AND a.scene_at IS NOT NULL
                                AND a.scene_at < p.file_mtime))
                     ORDER BY (p.capture_datetime IS NULL) ASC,
                              COALESCE(p.capture_datetime, p.indexed_at), p.id",
                )
                .map_err(db_err("prepare scenes queue"))?;
            for row in stmt.query_map(params![sid], |r| {
                    Ok(SceneWork {
                        photo_id: r.get(0)?,
                        path: r.get(1)?,
                        file_mtime: r.get(2)?,
                    })
                }).map_err(db_err("query scenes queue"))? {
                out.push(row.map_err(db_err("read scene work row"))?);
            }
        } else {
            let mut stmt = conn
                .prepare(
                    "SELECT p.id, p.path, p.file_mtime
                     FROM photos p
                     LEFT JOIN analysis a ON a.photo_id = p.id
                     WHERE p.extension IN ('jpg', 'jpeg', 'png', 'webp', 'tif', 'tiff')
                       AND (a.scene_fine IS NULL
                            OR (p.file_mtime IS NOT NULL AND a.scene_at IS NOT NULL
                                AND a.scene_at < p.file_mtime))
                     ORDER BY (p.capture_datetime IS NULL) ASC,
                              COALESCE(p.capture_datetime, p.indexed_at), p.id",
                )
                .map_err(db_err("prepare scenes queue"))?;
            for row in stmt.query_map([], |r| {
                    Ok(SceneWork {
                        photo_id: r.get(0)?,
                        path: r.get(1)?,
                        file_mtime: r.get(2)?,
                    })
                }).map_err(db_err("query scenes queue"))? {
                out.push(row.map_err(db_err("read scene work row"))?);
            }
        }
        Ok(out)
    }

    /// Store one photo's scene result (Sprint 18). Same face-only-row rule:
    /// a missing `analysis` row gets created with measurements still NULL.
    pub fn upsert_scene(
        &self,
        photo_id: i64,
        coarse: &str,
        fine: &str,
        conf: f64,
        source_mtime: Option<&str>,
    ) -> AppResult<()> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO analysis (photo_id, scene_coarse, scene_fine, scene_conf, scene_at, analyzed_at, algorithm_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)
             ON CONFLICT(photo_id) DO UPDATE SET
                scene_coarse = excluded.scene_coarse,
                scene_fine = excluded.scene_fine,
                scene_conf = excluded.scene_conf,
                scene_at = excluded.scene_at",
            params![photo_id, coarse, fine, conf, source_mtime, crate::time::now_utc()],
        )
        .map_err(db_err("upsert scene"))?;
        Ok(())
    }

    /// How many photos carry a scene result (the Settings line).
    pub fn scenes_done(&self) -> AppResult<i64> {
        let conn = self.lock()?;
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM analysis WHERE scene_fine IS NOT NULL", [], |r| {
                r.get(0)
            })
            .map_err(db_err("count scenes done"))?;
        Ok(n)
    }

    /// Store one photo's face result. A photo without an `analysis` row gets
    /// a face-only row (measurements stay NULL until the analysis pass runs;
    /// `analyzed_at`/`algorithm_version` are filled so the NOT NULL
    /// constraints hold — the row then re-enters the analysis queue
    /// naturally via the NULL-safe `source_mtime` comparison). The update
    /// path never touches columns the analysis pass owns, and vice versa.
    pub fn upsert_faces(
        &self,
        photo_id: i64,
        face_count: i64,
        source_mtime: Option<&str>,
    ) -> AppResult<()> {
        self.upsert_faces_with_observations(photo_id, face_count, &[], source_mtime)
    }

    /// Store a face count and compact local appearance signatures together.
    /// A signature is a perceptual hash of one detected face crop, never an
    /// image or a cloud-recognisable embedding. Replacing rows prevents stale
    /// observations after a photo changes on disk.
    pub fn upsert_faces_with_observations(
        &self,
        photo_id: i64,
        face_count: i64,
        appearance_hashes: &[u64],
        source_mtime: Option<&str>,
    ) -> AppResult<()> {
        let conn = self.lock()?;
        let tx = conn.unchecked_transaction().map_err(db_err("face observations tx"))?;
        tx.execute(
            "INSERT INTO analysis (photo_id, face_count, faces_at, analyzed_at, algorithm_version)
             VALUES (?1, ?2, ?3, ?4, 1)
             ON CONFLICT(photo_id) DO UPDATE SET
                face_count = excluded.face_count,
                faces_at = excluded.faces_at",
            params![photo_id, face_count, source_mtime, crate::time::now_utc()],
        )
        .map_err(db_err("upsert faces"))?;
        tx.execute("DELETE FROM face_observations WHERE photo_id = ?1", [photo_id])
            .map_err(db_err("clear face observations"))?;
        for (face_index, hash) in appearance_hashes.iter().enumerate() {
            tx.execute(
                "INSERT INTO face_observations (photo_id, face_index, appearance_hash, source_mtime)
                 VALUES (?1, ?2, ?3, ?4)",
                params![photo_id, face_index as i64, *hash as i64, source_mtime],
            )
            .map_err(db_err("insert face observation"))?;
        }
        tx.commit().map_err(db_err("commit face observations"))?;
        Ok(())
    }

    /// Stored local face-appearance hashes for one project. Only photos in
    /// the requested session are returned, so portrait candidates cannot
    /// bridge into another project.
    pub fn face_observations_for_session(&self, session_id: i64) -> AppResult<Vec<(i64, u64)>> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT o.photo_id, o.appearance_hash
                 FROM face_observations o
                 JOIN photos p ON p.id = o.photo_id
                 WHERE p.session_id = ?1",
            )
            .map_err(db_err("prepare project face observations"))?;
        let rows = stmt
            .query_map([session_id], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)? as u64))
            })
            .map_err(db_err("query project face observations"))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(db_err("read project face observation"))?);
        }
        Ok(out)
    }

    /// How many photos carry a face result (the Settings "N of M" line).
    pub fn faces_done(&self) -> AppResult<i64> {
        let conn = self.lock()?;
        conn.query_row(
            "SELECT COUNT(*) FROM analysis WHERE face_count IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .map_err(db_err("count faces done"))
    }
}

// ---------------------------------------------------------------------------
// Sprint 8 row types (saved views, collections, similarity, phash)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
pub struct SavedView {
    pub id: i64,
    pub name: String,
    pub filter_json: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Collection {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,
    pub photo_count: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SimilarityGroup {
    pub id: i64,
    pub hash: String,
    pub group_type: String,
    pub photo_count: i64,
    pub created_at: String,
    /// Distinct sessions spanned (1 = within one shoot; ≥ 2 = cross-session,
    /// Sprint 16 — photos were imported into different sessions).
    pub session_count: i64,
    /// Up to 4 photo ids (by id order) for a UI cover strip.
    pub cover_photos: Vec<i64>,
}

#[derive(Debug, Clone)]
pub struct PhashWork {
    pub photo_id: i64,
    pub path: String,
    pub extension: String,
    pub file_mtime: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FaceWork {
    pub photo_id: i64,
    pub path: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub file_mtime: Option<String>,
}

/// One queued unit of scene-classification work (Sprint 18): a decodable
/// photo missing a scene result, or whose file changed since the last run.
pub struct SceneWork {
    pub photo_id: i64,
    pub path: String,
    pub file_mtime: Option<String>,
}

/// One queued unit of analysis work: a decodable photo whose row is missing,
/// stale (older algorithm), or computed from a different file mtime.
/// One queued unit of metadata work: a photo the EXIF/metadata pass hasn't
/// read yet (`exif_at IS NULL`). Carries the dimensions the scanner already
/// resolved so the pass only derives orientation where it fills them.
#[derive(Debug, Clone)]
pub struct ExifWork {
    pub photo_id: i64,
    pub path: String,
    pub extension: String,
    pub filename: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub orientation: Option<String>,
    /// Stored file mtime (the date-estimation fallback source).
    pub file_mtime: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AnalysisWork {
    pub photo_id: i64,
    pub path: String,
    /// mtime recorded when the file was indexed; compared against the row's
    /// `source_mtime` for incremental re-analysis.
    pub file_mtime: Option<String>,
    pub filename: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DbStatus {
    pub photo_count: i64,
    pub session_count: i64,
    pub analyzed_count: i64,
    /// Photos not yet read by the EXIF/metadata pass (`exif_at IS NULL`).
    pub metadata_pending: i64,
    /// Culling state (Sprint 7): photos marked selected / rejected.
    pub selected_count: i64,
    pub rejected_count: i64,
    /// Photos with a local-AI face result (Sprint 9; `face_count` set).
    pub faces_done: i64,
    /// Photos with a scene-classification result (Sprint 18).
    pub scenes_done: i64,
    pub schema_version: i64,
}



// ---------------------------------------------------------------------------
// file_operations audit log (Sprint 7)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
pub struct FileOpRow {
    pub id: i64,
    pub op_type: String,
    pub source_path: String,
    pub dest_path: Option<String>,
    /// "done" | "failed" (v0.1 writes exactly these).
    pub status: String,
    pub detail: Option<String>,
    pub created_at: String,
}



// ---------------------------------------------------------------------------
// Selection state (Sprint 7; reads since Sprint 6)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
pub struct SelectionRow {
    pub photo_id: i64,
    pub state: String,
    pub updated_at: String,
}

const SELECTION_STATES: [&str; 3] = ["selected", "rejected", "needs_attention"];







// ---------------------------------------------------------------------------

/// Inputs for a photo row upsert (scan-time fields only; EXIF fields are
/// filled by the metadata step and must never be clobbered here).
#[derive(Debug, Clone)]
pub struct PhotoUpsert {
    pub path: String,
    pub filename: String,
    pub extension: String,
    pub size_bytes: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub orientation: Option<String>,
    /// Session the photo belongs to; NULL for copies into non-library
    /// folders (the row stays reachable via the file-operations audit log).
    pub session_id: Option<i64>,
    pub file_mtime: Option<String>,
}

/// Grid tile row: just enough to render a thumbnail tile and open the
/// viewer. The full record (path, EXIF, analysis) arrives via
/// `get_photo_full` — keeping page payloads small for 10k-photo libraries.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PhotoSummary {
    pub id: i64,
    pub filename: String,
    pub extension: String,
    pub size_bytes: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub orientation: Option<String>,
    pub capture_datetime: Option<String>,
    pub session_id: Option<i64>,
    pub has_analysis: bool,
    /// Curatorial marks (Sprint 13). rating NULL = unrated (0..5 when
    /// set); flag 0/1; color_label NULL | fixed enum.
    pub rating: Option<i64>,
    pub flag: bool,
    pub color_label: Option<String>,
}

/// `PhotoPage` is what `list_photos` returns over IPC.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PhotoPage {
    pub photos: Vec<PhotoSummary>,
    pub total: i64,
}

/// One concrete local metadata value and its in-scope photograph count.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FilterValueOption {
    pub value: String,
    pub count: i64,
}

/// Values offered by metadata-backed filter fields for the active shoot.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FilterValueOptions {
    pub values: Vec<FilterValueOption>,
    pub unidentified_count: i64,
}

/// Local availability for one numeric filter field in the requested shoot.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NumericFilterStats {
    pub recorded_count: i64,
    pub missing_count: i64,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
}

/// One reusable similarity/burst decision unit for the session review UI.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReviewSequence {
    pub id: i64,
    pub group_type: String,
    pub photo_ids: Vec<i64>,
}

/// Ordered session photos and the existing local grouping information. The UI
/// decides presentation; this data object contains no aesthetic ranking.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReviewQueue {
    pub photos: Vec<PhotoSummary>,
    pub sequences: Vec<ReviewSequence>,
}

/// Full photo + its analysis row (NULLs when analysis hasn't run yet).
#[derive(Debug, Clone, serde::Serialize)]
pub struct PhotoFull {
    pub id: i64,
    pub path: String,
    pub filename: String,
    pub extension: String,
    pub size_bytes: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub orientation: Option<String>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    pub lens_make: Option<String>,
    pub software: Option<String>,
    /// Where the camera/exposure/date values came from: 'none' | 'exif'
    /// | 'filename' | 'mtime' (dominance order, see DATABASE.md).
    pub metadata_source: String,
    /// Provenance of `capture_datetime` itself: 'exif' | 'filename' |
    /// 'mtime' (NULL when no capture date exists at all).
    pub capture_datetime_source: Option<String>,
    pub focal_length: Option<f64>,
    pub iso: Option<i64>,
    pub aperture: Option<f64>,
    pub shutter_speed: Option<f64>,
    pub capture_datetime: Option<String>,
    pub gps_present: bool,
    pub session_id: Option<i64>,
    /// Curatorial marks (Sprint 13). rating NULL = unrated (0..5 when
    /// set); flag 0/1; color_label NULL | fixed enum.
    pub rating: Option<i64>,
    pub flag: bool,
    pub color_label: Option<String>,
    pub indexed_at: String,
    pub file_mtime: Option<String>,
    // ---- analysis (NULL before the analysis pass has run) ----
    pub sharpness: Option<f64>,
    pub brightness: Option<f64>,
    pub contrast: Option<f64>,
    pub saturation: Option<f64>,
    pub highlight_clipping: Option<f64>,
    pub shadow_clipping: Option<f64>,
    pub is_monochrome: bool,
    pub is_dark: bool,
    pub is_bright: bool,
    pub face_count: Option<i64>,
    pub smile_count: Option<i64>,
    pub scene_coarse: Option<String>,
    pub scene_fine: Option<String>,
    pub scene_conf: Option<f64>,
    pub perceptual_hash: Option<String>,
    pub algorithm_version: Option<u32>,
    pub analyzed_at: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionRow {
    pub id: i64,
    pub name: String,
    pub root_path: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub photo_count: i64,
    pub created_at: String,
}

fn db_err(context: &str) -> impl Fn(rusqlite::Error) -> AppError + 'static {
    let context = context.to_string();
    move |e| {
        tracing::error!(%context, error = %e, "sqlite error");
        AppError::Database(e.to_string())
    }
}

/// Whether `table` already declares `column` (used to keep `ALTER TABLE`
/// migrations idempotent across versions).
fn table_has_column(conn: &Connection, table: &str, column: &str) -> bool {
    conn.prepare(&format!("PRAGMA table_info({table})"))
        .ok()
        .and_then(|mut stmt| {
            let cols = stmt.query_map([], |r| r.get::<_, String>(1)).ok()?;
            Some(cols.into_iter().any(|c| c.as_deref().ok() == Some(column)))
        })
        .unwrap_or(false)
}

// Convenience: unit tests build a temp database.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_creates_expected_tables() {
        let dir = std::env::temp_dir().join(format!("pg_db_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let db_path = dir.join("test.sqlite");
        let _ = std::fs::remove_file(&db_path);

        let db = Db::open(&db_path).expect("open");
        let v = db.migrate().expect("migrate");
        assert_eq!(v, CURRENT_SCHEMA_VERSION);

        let tables: Vec<String> = {
            let conn = db.lock().unwrap();
            let mut stmt = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .unwrap();
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            rows
        };
        for expected in [
            "analysis",
            "app_settings",
            "collections",
            "collection_photos",
            "file_operations",
            "photos",
            "schema_version",
            "sessions",
            "saved_views",
            "similarity_groups",
            "similarity_group_photos",
        ] {
            assert!(tables.contains(&expected.to_string()), "missing table {expected}");
        }
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn migrate_is_idempotent() {
        let dir = std::env::temp_dir().join(format!("pg_db_test_idem_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let db_path = dir.join("test.sqlite");
        let _ = std::fs::remove_file(&db_path);

        let db = Db::open(&db_path).unwrap();
        db.migrate().unwrap();
        let v2 = db.migrate().unwrap();
        assert_eq!(v2, CURRENT_SCHEMA_VERSION);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn review_state_accepts_attention_without_redefining_existing_choices() {
        let dir = std::env::temp_dir().join(format!("pg_db_test_review_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let db_path = dir.join("test.sqlite");
        let _ = std::fs::remove_file(&db_path);
        let db = Db::open(&db_path).unwrap();
        db.migrate().unwrap();

        let id = db
            .upsert_photo(&PhotoUpsert {
                path: dir.join("review.jpg").display().to_string(),
                filename: "review.jpg".to_string(),
                extension: "jpg".to_string(),
                size_bytes: Some(1),
                width: Some(10),
                height: Some(10),
                orientation: None,
                session_id: None,
                file_mtime: None,
            })
            .unwrap();
        db.set_selection(id, "needs_attention").unwrap();
        assert_eq!(db.list_selections(10).unwrap()[0].state, "needs_attention");
        assert!(db.set_selection(id, "unknown").is_err());

        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn metadata_filter_values_are_scoped_and_include_unidentified() {
        let dir = std::env::temp_dir().join(format!("pg_db_test_filter_values_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let db_path = dir.join("test.sqlite");
        let _ = std::fs::remove_file(&db_path);
        let db = Db::open(&db_path).unwrap();
        db.migrate().unwrap();
        let first = db.upsert_session("First", Some("/first")).unwrap();
        let second = db.upsert_session("Second", Some("/second")).unwrap();

        for (name, session) in [("a.jpg", first), ("b.jpg", first), ("c.jpg", second)] {
            db.upsert_photo(&PhotoUpsert {
                path: dir.join(name).display().to_string(),
                filename: name.to_string(),
                extension: "jpg".to_string(),
                size_bytes: Some(1),
                width: Some(10),
                height: Some(10),
                orientation: None,
                session_id: Some(session),
                file_mtime: None,
            }).unwrap();
        }
        {
            let conn = db.lock().unwrap();
            conn.execute("UPDATE photos SET camera_make = 'Canon' WHERE filename = 'a.jpg'", []).unwrap();
            conn.execute("UPDATE photos SET camera_make = 'Sony' WHERE filename = 'c.jpg'", []).unwrap();
        }

        let scoped = db.filter_value_options("camera_make", Some(first)).unwrap();
        assert_eq!(scoped.values.len(), 1);
        assert_eq!(scoped.values[0].value, "Canon");
        assert_eq!(scoped.values[0].count, 1);
        assert_eq!(scoped.unidentified_count, 1);
        assert!(db.filter_value_options("filename", Some(first)).is_err());

        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn numeric_filter_stats_are_scoped_and_keep_missing_explicit() {
        let dir = std::env::temp_dir().join(format!(
            "pg_db_test_numeric_filter_stats_{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let db_path = dir.join("test.sqlite");
        let _ = std::fs::remove_file(&db_path);
        let db = Db::open(&db_path).unwrap();
        db.migrate().unwrap();
        let first = db.upsert_session("First", Some("/first-stats")).unwrap();
        let second = db.upsert_session("Second", Some("/second-stats")).unwrap();

        let mut ids = Vec::new();
        for (name, session) in [("a.jpg", first), ("b.jpg", first), ("c.jpg", second)] {
            ids.push(
                db.upsert_photo(&PhotoUpsert {
                    path: dir.join(name).display().to_string(),
                    filename: name.to_string(),
                    extension: "jpg".to_string(),
                    size_bytes: Some(1),
                    width: Some(10),
                    height: Some(10),
                    orientation: None,
                    session_id: Some(session),
                    file_mtime: None,
                })
                .unwrap(),
            );
        }
        {
            let conn = db.lock().unwrap();
            conn.execute(
                "UPDATE photos SET iso = 400, focal_length = 50 WHERE id = ?1",
                [ids[0]],
            )
            .unwrap();
            conn.execute("UPDATE photos SET iso = 1600 WHERE id = ?1", [ids[2]])
                .unwrap();
        }
        db.upsert_analysis(
            ids[0],
            &crate::analysis::metrics::Metrics {
                sharpness: 72.0,
                brightness: 31.0,
                contrast: 55.0,
                saturation: 40.0,
                highlight_clipping: 0.0,
                shadow_clipping: 1.0,
                is_monochrome: false,
                is_dark: true,
                is_bright: false,
            },
            None,
        )
        .unwrap();

        let iso = db.numeric_filter_stats("iso", Some(first)).unwrap();
        assert_eq!(iso.recorded_count, 1);
        assert_eq!(iso.missing_count, 1);
        assert_eq!(iso.minimum, Some(400.0));
        assert_eq!(iso.maximum, Some(400.0));
        let brightness = db.numeric_filter_stats("brightness", Some(first)).unwrap();
        assert_eq!(brightness.recorded_count, 1);
        assert_eq!(brightness.missing_count, 1);
        assert_eq!(brightness.minimum, Some(31.0));
        assert!(db.numeric_filter_stats("filename", Some(first)).is_err());

        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_dir(&dir);
    }

    /// v13: curatorial marks — columns exist, batch set/clear works, and
    /// invalid values fail before touching the database.
    #[test]
    fn marks_columns_and_batch_semantics() {
        let dir = std::env::temp_dir().join(format!("pg_db_test_marks_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let db_path = dir.join("test.sqlite");
        let _ = std::fs::remove_file(&db_path);

        let db = Db::open(&db_path).unwrap();
        db.migrate().unwrap();
        {
            let conn = db.lock().unwrap();
            for col in ["rating", "flag", "color_label"] {
                assert!(table_has_column(&conn, "photos", col), "missing {col}");
            }
        }

        let mut ids = Vec::new();
        for name in ["a.jpg", "b.jpg", "c.jpg"] {
            ids.push(
                db.upsert_photo(&PhotoUpsert {
                    path: dir.join(name).display().to_string(),
                    filename: name.to_string(),
                    extension: "jpg".to_string(),
                    size_bytes: Some(1),
                    width: Some(10),
                    height: Some(10),
                    orientation: None,
                    session_id: None,
                    file_mtime: None,
                })
                .unwrap(),
            );
        }

        // Batch set: stars + flag + color on two photos at once.
        let n = db.set_marks(&ids[..2], Some(4), Some(true), Some("red")).unwrap();
        assert_eq!(n, 2);
        // Clear the flag only on the first photo; rating/color untouched.
        db.set_marks(&ids[..1], None, Some(false), None).unwrap();
        // Unrate + clear color on the first photo.
        db.set_marks(&ids[..1], Some(0), None, Some("")).unwrap();

        let p1 = db.get_photo_full(ids[0]).unwrap();
        assert_eq!(p1.rating, None);
        assert!(!p1.flag);
        assert_eq!(p1.color_label, None);
        let p2 = db.get_photo_full(ids[1]).unwrap();
        assert_eq!(p2.rating, Some(4));
        assert!(p2.flag);
        assert_eq!(p2.color_label.as_deref(), Some("red"));
        let p3 = db.get_photo_full(ids[2]).unwrap();
        assert_eq!(p3.rating, None);
        assert!(!p3.flag);
        assert_eq!(p3.color_label, None);

        // Validation: out-of-range rating and unknown color fail; empty id
        // list and a no-op call are harmless.
        assert!(db.set_marks(&ids, Some(7), None, None).is_err());
        assert!(db.set_marks(&ids, None, None, Some("mauve")).is_err());
        assert_eq!(db.set_marks(&[], Some(3), None, None).unwrap(), 0);
        assert_eq!(db.set_marks(&ids, None, None, None).unwrap(), 0);

        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_dir(&dir);
    }

    /// v11 column set + the incremental EXIF queue: a photo is queued exactly
    /// twice — once before any read, once when its file gets newer than the
    /// last read — and never again while the file is unchanged.
    #[test]
    fn exif_queue_is_incremental_and_v11_columns_exist() {
        let dir = std::env::temp_dir().join(format!("pg_db_test_exifq_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let db_path = dir.join("test.sqlite");
        let _ = std::fs::remove_file(&db_path);

        let db = Db::open(&db_path).unwrap();
        db.migrate().unwrap();

        let photo_id = db
            .upsert_photo(&PhotoUpsert {
                path: dir.join("old.jpg").display().to_string(),
                filename: "old.jpg".to_string(),
                extension: "jpg".to_string(),
                size_bytes: Some(1000),
                width: Some(4000),
                height: Some(3000),
                orientation: Some("landscape".to_string()),
                session_id: None,
                file_mtime: Some("2026-01-01T00:00:00Z".to_string()),
            })
            .unwrap();

        // Never read: queued, and reported pending.
        assert_eq!(db.exif_queue().unwrap().len(), 1);
        assert_eq!(db.status().unwrap().metadata_pending, 1);

        // First read lands metadata; source escalates from 'none' to 'exif'.
        let rec = crate::metadata::ExifRecord {
            camera_make: Some("CamCo".to_string()),
            iso: Some(200),
            lens_make: Some("OptCo".to_string()),
            software: Some("Editor 2.0".to_string()),
            ..Default::default()
        };
        db.upsert_exif(photo_id, &rec, None).unwrap();

        let full = db.get_photo_full(photo_id).unwrap();
        assert_eq!(full.camera_make.as_deref(), Some("CamCo"));
        assert_eq!(full.lens_make.as_deref(), Some("OptCo"));
        assert_eq!(full.software.as_deref(), Some("Editor 2.0"));
        assert_eq!(full.metadata_source, "exif");

        // A photo change re-read: capture date estimated (filename) when EXIF
        // carries none; the estimate is labelled, EXIF still dominates
        // `metadata_source`.
        let est = crate::metadata::DateEstimate {
            datetime: "2026-03-04T11:22:33Z".to_string(),
            source: "filename",
        };
        db.upsert_exif(photo_id, &rec, Some(&est)).unwrap();
        let full = db.get_photo_full(photo_id).unwrap();
        assert_eq!(full.capture_datetime.as_deref(), Some("2026-03-04T11:22:33Z"));
        assert_eq!(full.capture_datetime_source.as_deref(), Some("filename"));
        assert_eq!(full.metadata_source, "exif");

        // Real EXIF date beats the estimate on the next read, and source
        // labels switch to 'exif'.
        let rec3 = crate::metadata::ExifRecord {
            camera_make: Some("CamCo".to_string()),
            capture_datetime: Some("2026-03-05T08:00:00Z".to_string()),
            ..Default::default()
        };
        db.upsert_exif(photo_id, &rec3, Some(&est)).unwrap();
        let full = db.get_photo_full(photo_id).unwrap();
        assert_eq!(full.capture_datetime.as_deref(), Some("2026-03-05T08:00:00Z"));
        assert_eq!(full.capture_datetime_source.as_deref(), Some("exif"));

        // An empty read (readable image, no EXIF) never marks source as
        // EXIF, and — once stamped — a re-read does not erase values.
        db.upsert_exif(photo_id, &crate::metadata::ExifRecord::default(), None)
            .unwrap();
        let full = db.get_photo_full(photo_id).unwrap();
        assert_eq!(full.camera_make.as_deref(), Some("CamCo"));
        assert_eq!(full.metadata_source, "exif");

        // Unchanged file: queue empty, pending zero.
        assert_eq!(db.exif_queue().unwrap().len(), 0);
        assert_eq!(db.status().unwrap().metadata_pending, 0);

        // The file changes on disk (newer mtime than the read stamp):
        // queued again and pending again. (Stamp is a real "now", so the
        // changed file's mtime must be in the future of this test run.)
        db.upsert_photo(&PhotoUpsert {
            path: dir.join("old.jpg").display().to_string(),
            filename: "old.jpg".to_string(),
            extension: "jpg".to_string(),
            size_bytes: Some(1001),
            width: Some(4000),
            height: Some(3000),
            orientation: Some("landscape".to_string()),
            session_id: None,
            file_mtime: Some("2099-01-02T00:00:00Z".to_string()),
        })
        .unwrap();
        assert_eq!(db.exif_queue().unwrap().len(), 1);
        assert_eq!(db.status().unwrap().metadata_pending, 1);

        // Re-read refreshes values (new read wins over the old one)…
        let rec2 = crate::metadata::ExifRecord {
            camera_make: Some("NewCam".to_string()),
            iso: Some(400),
            ..Default::default()
        };
        db.upsert_exif(photo_id, &rec2, None).unwrap();
        let full = db.get_photo_full(photo_id).unwrap();
        assert_eq!(full.camera_make.as_deref(), Some("NewCam"));
        assert_eq!(full.iso, Some(400));
        // …while untouched fields survive the re-read (None never erases).
        assert_eq!(full.lens_make.as_deref(), Some("OptCo"));
        assert_eq!(full.software.as_deref(), Some("Editor 2.0"));

        // With the file now older than the re-read stamp (in real life the
        // stamp is the read time, ≥ the file's write time), it drops out.
        db.upsert_photo(&PhotoUpsert {
            path: dir.join("old.jpg").display().to_string(),
            filename: "old.jpg".to_string(),
            extension: "jpg".to_string(),
            size_bytes: Some(1001),
            width: Some(4000),
            height: Some(3000),
            orientation: Some("landscape".to_string()),
            session_id: None,
            file_mtime: Some("2000-01-01T00:00:00Z".to_string()),
        })
        .unwrap();
        assert_eq!(db.exif_queue().unwrap().len(), 0);

        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_dir(&dir);
    }
}
