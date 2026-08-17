//! SQLite access. The catalog lives entirely on the user's machine.
//!
//! Concurrency model: a single `Connection` guarded by a `Mutex`. All
//! critical sections are short (no awaits while holding the lock).
//!
//! Schema is versioned via `schema_version` and applied incrementally.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{AppError, AppResult};

pub const CURRENT_SCHEMA_VERSION: i64 = 5;

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

    /// Quick status counters for the shell UI.
    pub fn status(&self) -> Result<DbStatus, AppError> {
        let conn = self.lock()?;
        let photo_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM photos", [], |r| r.get(0))
            .map_err(db_err("count photos"))?;
        let session_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .map_err(db_err("count sessions"))?;
        let analyzed_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM analysis", [], |r| r.get(0))
            .map_err(db_err("count analysis"))?;
        let version: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_version",
                [],
                |r| r.get(0),
            )
            .map_err(db_err("read version"))?;
        Ok(DbStatus {
            photo_count,
            session_count,
            analyzed_count,
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
            .query_map([], |r| {
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
    pub fn refresh_session_counts(&self, session_id: i64) -> AppResult<()> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE sessions
             SET photo_count = (SELECT COUNT(*) FROM photos WHERE session_id = ?1)
             WHERE id = ?1",
            params![session_id],
        )
        .map_err(db_err("refresh session counts"))?;
        Ok(())
    }

    /// Paginated photo list for the grid. Lightweight columns only — the
    /// grid needs enough to render tiles and drive the viewer, not the full
    /// analysis blob (fetched on demand via `get_photo_full`).
    pub fn list_photos(
        &self,
        offset: i64,
        limit: i64,
    ) -> AppResult<(Vec<PhotoSummary>, i64)> {
        let conn = self.lock()?;
        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM photos", [], |r| r.get(0))
            .map_err(db_err("count photos"))?;

        let limit = limit.clamp(1, 500);
        let offset = offset.max(0);
        let mut stmt = conn
            .prepare(
                "SELECT p.id, p.filename, p.extension, p.size_bytes, p.width, p.height,
                        p.orientation, p.capture_datetime, p.session_id,
                        (a.photo_id IS NOT NULL) AS has_analysis
                 FROM photos p
                 LEFT JOIN analysis a ON a.photo_id = p.id
                 ORDER BY (p.capture_datetime IS NULL) ASC, p.capture_datetime ASC, p.id ASC
                 LIMIT ?1 OFFSET ?2",
            )
            .map_err(db_err("prepare list_photos"))?;
        let rows = stmt
            .query_map(params![limit, offset], |r| {
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
                })
            })
            .map_err(db_err("query list_photos"))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(db_err("read photo row"))?);
        }
        Ok((out, total))
    }

    /// One full photo with its analysis row (for the viewer metadata panel).
    pub fn get_photo_full(&self, id: i64) -> AppResult<PhotoFull> {
        let conn = self.lock()?;
        let row: Option<PhotoFull> = conn
            .query_row(
                "SELECT p.id, p.path, p.filename, p.extension, p.size_bytes, p.width, p.height,
                        p.orientation, p.camera_make, p.camera_model, p.lens, p.focal_length,
                        p.iso, p.aperture, p.shutter_speed, p.capture_datetime, p.gps_present,
                        p.session_id, p.indexed_at, p.file_mtime,
                        a.sharpness, a.brightness, a.contrast, a.saturation,
                        a.highlight_clipping, a.shadow_clipping, a.is_monochrome, a.is_dark,
                        a.is_bright, a.face_count, a.smile_count, a.perceptual_hash,
                        a.algorithm_version, a.analyzed_at
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
                        focal_length: r.get(11)?,
                        iso: r.get(12)?,
                        aperture: r.get(13)?,
                        shutter_speed: r.get(14)?,
                        capture_datetime: r.get(15)?,
                        gps_present: r.get::<_, i64>(16)? != 0,
                        session_id: r.get(17)?,
                        indexed_at: r.get(18)?,
                        file_mtime: r.get(19)?,
                        sharpness: r.get(20)?,
                        brightness: r.get(21)?,
                        contrast: r.get(22)?,
                        saturation: r.get(23)?,
                        highlight_clipping: r.get(24)?,
                        shadow_clipping: r.get(25)?,
                        is_monochrome: r.get::<_, Option<i64>>(26)?.unwrap_or(0) != 0,
                        is_dark: r.get::<_, Option<i64>>(27)?.unwrap_or(0) != 0,
                        is_bright: r.get::<_, Option<i64>>(28)?.unwrap_or(0) != 0,
                        face_count: r.get(29)?,
                        smile_count: r.get(30)?,
                        perceptual_hash: r.get(31)?,
                        algorithm_version: r.get(32)?,
                        analyzed_at: r.get(33)?,
                    })
                },
            )
            .optional()
            .map_err(db_err("get_photo_full"))?;
        row.ok_or_else(|| AppError::operation("This photograph is no longer in the library"))
    }

    pub fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, AppError> {
        self.conn
            .lock()
            .map_err(|e| AppError::Database(format!("database lock poisoned: {e}")))
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DbStatus {
    pub photo_count: i64,
    pub session_count: i64,
    pub analyzed_count: i64,
    pub schema_version: i64,
}

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
    pub session_id: i64,
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
}

/// `PhotoPage` is what `list_photos` returns over IPC.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PhotoPage {
    pub photos: Vec<PhotoSummary>,
    pub total: i64,
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
    pub focal_length: Option<f64>,
    pub iso: Option<i64>,
    pub aperture: Option<f64>,
    pub shutter_speed: Option<f64>,
    pub capture_datetime: Option<String>,
    pub gps_present: bool,
    pub session_id: Option<i64>,
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
}
