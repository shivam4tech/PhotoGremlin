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

/// Resolve the persisted active folder, dropping it (and the setting) when
/// the folder no longer exists on disk, so a restart never resurrects a
/// deleted or moved path.
pub fn resolve_active_folder(db: &crate::database::Db) -> AppResult<Option<String>> {
    match db.get_setting(SETTING_ACTIVE_FOLDER)? {
        Some(p) if std::path::Path::new(&p).is_dir() => Ok(Some(p)),
        Some(_) => {
            db.clear_setting(SETTING_ACTIVE_FOLDER)?;
            Ok(None)
        }
        None => Ok(None),
    }
}

#[tauri::command]
pub fn get_active_folder(state: State<AppState>) -> AppResult<Option<String>> {
    resolve_active_folder(&state.db)
}

/// Open a native folder picker. Runs the dialog via the dialog plugin so GTK
/// thread-ownership stays correct. Returns `None` when the user cancels.
///
/// The command is async: the dialog plugin fires the callback on the main GTK
/// loop once the user picks or cancels, and we await that result on the async
/// runtime without ever blocking the main thread (a sync command here would
/// freeze the UI and crash the app). A timeout protects against a dialog
/// backend that never fires the callback.
#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel::<Option<String>>();
    app.dialog().file().pick_folder(move |selection| {
        let _ = tx.send(selection.map(|p| p.to_string()));
    });
    match tokio::time::timeout(std::time::Duration::from_secs(15 * 60), rx).await {
        Ok(Ok(path)) => path,
        _ => None,
    }
}
