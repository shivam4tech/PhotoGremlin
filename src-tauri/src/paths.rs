//! Resolves PhotoGremlin's local data locations (OS-conventional, privacy-first).
//!
//!   data_dir        ~/.local/share/com.photogremlin.app   (Linux example)
//!   cache_dir       ~/.cache/com.photogremlin.app
//!   log_dir         <data_dir>/logs                        (Tauri log dir)
//!
//! Layout:
//!   data_dir/database.sqlite        - the local catalog
//!   cache_dir/thumbnails/           - generated thumbnails
//!   log_dir/photogremlin.<date>.log - rolling local log

use std::path::PathBuf;

pub struct AppPaths {
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub log_dir: PathBuf,
}

impl AppPaths {
    /// Build paths from Tauri's path resolver (used at app startup).
    pub fn from_resolver(resolver: &tauri::path::PathResolver<tauri::Wry>) -> Self {
        let data_dir = resolver
            .app_data_dir()
            .expect("app data dir must resolve");
        let cache_dir = resolver
            .app_cache_dir()
            .expect("app cache dir must resolve");
        let log_dir = resolver.app_log_dir().expect("app log dir must resolve");
        Self {
            data_dir,
            cache_dir,
            log_dir,
        }
    }

    pub fn from_dirs(data_dir: PathBuf, cache_dir: PathBuf, log_dir: PathBuf) -> Self {
        Self {
            data_dir,
            cache_dir,
            log_dir,
        }
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("database.sqlite")
    }

    pub fn thumbnails_dir(&self) -> PathBuf {
        self.cache_dir.join("thumbnails")
    }

    pub fn log_path(&self) -> PathBuf {
        self.log_dir.join("photogremlin.log")
    }

    pub fn catalogs_dir(&self) -> PathBuf {
        self.data_dir.join("catalogs")
    }

    pub fn catalog_db_path(&self, slug: &str) -> PathBuf {
        self.catalogs_dir().join(format!("{slug}.sqlite"))
    }

    pub fn catalog_backups_dir(&self) -> PathBuf {
        self.data_dir.join("catalog-backups")
    }

    /// Ensure all directories exist. Returns the first error if any fail.
    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(&self.cache_dir)?;
        std::fs::create_dir_all(self.thumbnails_dir())?;
        std::fs::create_dir_all(&self.log_dir)?;
        std::fs::create_dir_all(self.catalogs_dir())?;
        std::fs::create_dir_all(self.catalog_backups_dir())?;
        Ok(())
    }
}
