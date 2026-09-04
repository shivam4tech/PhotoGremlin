//! Shared application state managed by Tauri.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use crate::database::Db;
use crate::error::{AppError, AppResult};
use crate::paths::AppPaths;
use crate::thumbnailer::ThumbService;
use crate::metadata::PauseControl;

/// Live background job handle (scan, analysis). Commands use `running` as a
/// claim and `cancel` as the cooperative stop flag checked between items by
/// the pipeline.
#[derive(Clone)]
pub struct Job {
    pub running: Arc<std::sync::atomic::AtomicBool>,
    pub cancel: Arc<std::sync::atomic::AtomicBool>,
    /// Metadata uses this gate between files; other jobs leave it idle.
    pub pause: PauseControl,
}

impl Job {
    pub fn new() -> Self {
        Self {
            running: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            pause: PauseControl::new(),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    /// Global preferences and the project-to-catalog registry. This database
    /// may also be the preserved catalog for the project that was active when
    /// a legacy single-catalog installation first upgraded.
    pub settings_db: Arc<Db>,
    /// The active catalog is swapped atomically after the replacement has
    /// opened, passed its integrity check, and completed migrations.
    active_catalog: Arc<RwLock<ActiveCatalog>>,
    pub paths: Arc<AppPaths>,
    /// Single scan slot (claim-and-cancel for folder scans).
    pub scan: Arc<Mutex<Option<Arc<Job>>>>,
    /// Single analysis slot (claim-and-cancel for the analysis pass).
    pub analysis: Arc<Mutex<Option<Arc<Job>>>>,
    /// Single metadata slot (claim-and-cancel for the EXIF pass).
    pub metadata: Arc<Mutex<Option<Arc<Job>>>>,
    /// Single file-operation slot (rename/move/copy/trash/permanent delete).
    pub operation: Arc<Mutex<Option<Arc<Job>>>>,
    /// Single similarity slot (claim-and-cancel for hashing + grouping).
    pub similarity: Arc<Mutex<Option<Arc<Job>>>>,
    /// Single face-detection slot (claim-and-cancel for the local-AI pass).
    pub faces: Arc<Mutex<Option<Arc<Job>>>>,
    /// Single scene-classification slot (claim-and-cancel; Sprint 18).
    pub scenes: Arc<Mutex<Option<Arc<Job>>>>,
    /// Single contact-sheet export slot (claim-and-cancel for PNG exports).
    pub export: Arc<Mutex<Option<Arc<Job>>>>,
    pub thumb: Arc<ThumbService>,
}

#[derive(Clone)]
struct ActiveCatalog {
    db: Arc<Db>,
    path: PathBuf,
}

impl AppState {
    pub fn new(
        settings_db: Arc<Db>,
        catalog: Arc<Db>,
        catalog_path: PathBuf,
        paths: Arc<AppPaths>,
        thumb: Arc<ThumbService>,
    ) -> Self {
        Self {
            settings_db,
            active_catalog: Arc::new(RwLock::new(ActiveCatalog {
                db: catalog,
                path: catalog_path,
            })),
            paths,
            scan: Arc::new(Mutex::new(None)),
            analysis: Arc::new(Mutex::new(None)),
            metadata: Arc::new(Mutex::new(None)),
            operation: Arc::new(Mutex::new(None)),
            similarity: Arc::new(Mutex::new(None)),
            faces: Arc::new(Mutex::new(None)),
            scenes: Arc::new(Mutex::new(None)),
            export: Arc::new(Mutex::new(None)),
            thumb,
        }
    }

    /// Capture the active catalog for one command/background job. A job keeps
    /// this Arc for its lifetime, so it can never drift into another project.
    pub fn db(&self) -> AppResult<Arc<Db>> {
        self.active_catalog
            .read()
            .map(|active| active.db.clone())
            .map_err(|_| AppError::Database("active catalog lock is unavailable".into()))
    }

    pub fn catalog_path(&self) -> AppResult<PathBuf> {
        self.active_catalog
            .read()
            .map(|active| active.path.clone())
            .map_err(|_| AppError::Database("active catalog path lock is unavailable".into()))
    }

    pub fn switch_catalog(&self, db: Arc<Db>, path: PathBuf) -> AppResult<()> {
        self.ensure_jobs_idle()?;
        let mut active = self
            .active_catalog
            .write()
            .map_err(|_| AppError::Database("active catalog lock is unavailable".into()))?;
        *active = ActiveCatalog { db, path };
        Ok(())
    }

    /// Switching a catalog while a background worker still holds the prior
    /// catalog would make completion events appear in the wrong project.
    pub fn ensure_jobs_idle(&self) -> AppResult<()> {
        let slots = [
            &self.scan,
            &self.analysis,
            &self.metadata,
            &self.operation,
            &self.similarity,
            &self.faces,
            &self.scenes,
            &self.export,
        ];
        for slot in slots {
            let guard = slot
                .lock()
                .map_err(|_| AppError::operation("A background task lock is unavailable"))?;
            if guard
                .as_ref()
                .is_some_and(|job| job.running.load(std::sync::atomic::Ordering::Relaxed))
            {
                return Err(AppError::operation(
                    "Finish or stop background work before switching projects.",
                ));
            }
        }
        Ok(())
    }

    pub fn is_active_catalog(&self, path: &Path) -> AppResult<bool> {
        Ok(self.catalog_path()?.as_path() == path)
    }
}
