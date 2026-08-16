//! App-level commands: info, paths, database status, active library folder.

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::database::DbStatus;
use crate::error::AppResult;
use crate::state::AppState;

const SETTING_ACTIVE_FOLDER: &str = "active_folder";

#[derive(Serialize, Clone)]
pub struct AppInfo {
    pub name: &'static str,
    pub version: String,
    pub platform: String,
    pub privacy: &'static str,
    pub offline_only: bool,
}

#[derive(Serialize, Clone)]
pub struct PathsInfo {
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub log_dir: PathBuf,
    pub db_path: PathBuf,
    pub thumbnails_dir: PathBuf,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        name: "PhotoGremlin",
        version: app_version(),
        platform: std::env::consts::OS.to_string(),
        privacy: "All data stays on this computer. No account, no cloud, no telemetry.",
        offline_only: true,
    }
}

fn app_version() -> String {
    option_env!("CARGO_PKG_VERSION").unwrap_or("0.0.0").to_string()
}

#[tauri::command]
pub fn app_paths(state: State<AppState>) -> PathsInfo {
    let p = &state.paths;
    PathsInfo {
        data_dir: p.data_dir.clone(),
        cache_dir: p.cache_dir.clone(),
        log_dir: p.log_dir.clone(),
        db_path: p.db_path(),
        thumbnails_dir: p.thumbnails_dir(),
    }
}

#[tauri::command]
pub fn db_status(state: State<AppState>) -> AppResult<DbStatus> {
    state.db.status()
}

/// Persist the active library folder (a scan root).
#[tauri::command]
pub fn set_active_folder(state: State<AppState>, path: String) -> AppResult<()> {
    let p = std::path::Path::new(&path);
    if !p.is_dir() {
        return Err(crate::error::AppError::validation(format!(
            "Folder does not exist: {path}"
        )));
    }
    state
        .db
        .set_setting(SETTING_ACTIVE_FOLDER, &p.canonicalize().unwrap_or_else(|_| p.to_path_buf()).display().to_string())
}

#[tauri::command]
pub fn get_active_folder(state: State<AppState>) -> AppResult<Option<String>> {
    state.db.get_setting(SETTING_ACTIVE_FOLDER)
}

/// Open a native folder picker. Runs the dialog via the dialog plugin so GTK
/// thread-ownership stays correct. Returns `None` when the user cancels.
#[tauri::command]
pub fn pick_folder(app: AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = std::sync::mpsc::channel::<Option<String>>();
    let builder = app.dialog().file();
    // The plugin API is callback-based; bridge to a blocking channel so the
    // command can return a plain value.
    builder
        .pick_folder(move |selection| {
            let _ = tx.send(selection.map(|p| p.to_string()));
        });
    // Block briefly for user interaction.
    let mut result = None;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15 * 60);
    while std::time::Instant::now() < deadline {
        match rx.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok(v) => {
                result = v;
                break;
            }
            Err(_) => {}
        }
    }
    result
}
