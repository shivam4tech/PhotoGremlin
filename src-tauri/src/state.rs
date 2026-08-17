//! Shared application state managed by Tauri.

use std::sync::{Arc, Mutex};

use crate::database::Db;
use crate::paths::AppPaths;
use crate::thumbnailer::ThumbService;

/// Live scan job handle. Commands use `running` as a claim and `cancel`
/// as the cooperative stop flag checked between files by the scanner.
#[derive(Clone)]
pub struct ScanJob {
    pub running: Arc<std::sync::atomic::AtomicBool>,
    pub cancel: Arc<std::sync::atomic::AtomicBool>,
}

impl ScanJob {
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
    pub scan: Arc<Mutex<Option<Arc<ScanJob>>>>,
    pub thumb: Arc<ThumbService>,
}
