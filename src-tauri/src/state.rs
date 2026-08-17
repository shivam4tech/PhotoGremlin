//! Shared application state managed by Tauri.

use std::sync::{Arc, Mutex};

use crate::database::Db;
use crate::paths::AppPaths;
use crate::thumbnailer::ThumbService;

/// Live background job handle (scan, analysis). Commands use `running` as a
/// claim and `cancel` as the cooperative stop flag checked between items by
/// the pipeline.
#[derive(Clone)]
pub struct Job {
    pub running: Arc<std::sync::atomic::AtomicBool>,
    pub cancel: Arc<std::sync::atomic::AtomicBool>,
}

impl Job {
    pub fn new() -> Self {
        Self {
            running: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Db>,
    pub paths: Arc<AppPaths>,
    /// Single scan slot (claim-and-cancel for folder scans).
    pub scan: Arc<Mutex<Option<Arc<Job>>>>,
    /// Single analysis slot (claim-and-cancel for the analysis pass).
    pub analysis: Arc<Mutex<Option<Arc<Job>>>>,
    pub thumb: Arc<ThumbService>,
}
