//! Shared application state managed by Tauri.

use std::sync::Arc;

use crate::database::Db;
use crate::paths::AppPaths;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Db>,
    pub paths: Arc<AppPaths>,
}
