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

pub const CURRENT_SCHEMA_VERSION: i64 = 4;

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
