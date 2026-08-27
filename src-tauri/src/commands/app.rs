//! App-level commands: info, paths, database status, active library folder,
//! recent projects, project lifecycle, and dashboard layout.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::database::DbStatus;
use crate::error::AppResult;
use crate::state::AppState;

const SETTING_ACTIVE_FOLDER: &str = "active_folder";
const SETTING_RECENT_FOLDERS: &str = "recent_folders";
const SETTING_RECENT_FOLDERS_CAP: usize = 12;
const SETTING_DASHBOARD_LAYOUT: &str = "dashboard_layout";

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

/// Persist the active library folder (a scan root) and record it in recents.
#[tauri::command]
pub fn set_active_folder(state: State<AppState>, path: String) -> AppResult<()> {
    let p = Path::new(&path);
    if !p.is_dir() {
        return Err(crate::error::AppError::validation(format!(
            "Folder does not exist: {path}"
        )));
    }
    let canonical = p.canonicalize().unwrap_or_else(|_| p.to_path_buf()).display().to_string();
    state.db.set_setting(SETTING_ACTIVE_FOLDER, &canonical)?;
    let _ = touch_recent(&state.db, &canonical);
    Ok(())
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

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
    pub path: String,
    pub name: String,
    pub parent: String,
    #[serde(alias = "last_opened_at")]
    pub last_opened_at: String,
    #[serde(alias = "photo_count")]
    pub photo_count: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct DashboardLayout {
    pub hidden: Vec<String>,
    pub order: Vec<String>,
}

/// Build display metadata for a recent-project path.
fn recent_entry(path: &str, photo_count: i64) -> RecentProject {
    let p = Path::new(path);
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string());
    let parent = p
        .parent()
        .map(|pp| pp.to_string_lossy().into_owned())
        .unwrap_or_default();
    RecentProject {
        path: path.to_string(),
        name,
        parent,
        last_opened_at: crate::time::now_utc(),
        photo_count,
    }
}

/// MRU upsert for recent_folders: dedup, prepend, cap 12, persist.
fn touch_recent(db: &crate::database::Db, path: &str) -> AppResult<()> {
    let mut recents = load_recent_raw(db)?;
    // count photos for this path's session if possible
    let photo_count = db
        .status()
        .map(|s| s.photo_count)
        .unwrap_or(0);
    // remove existing entry for same path
    recents.retain(|r: &RecentProject| r.path != path);
    recents.insert(0, recent_entry(path, photo_count));
    if recents.len() > SETTING_RECENT_FOLDERS_CAP {
        recents.truncate(SETTING_RECENT_FOLDERS_CAP);
    }
    // refresh last_opened_at for the touched entry
    if let Some(first) = recents.first_mut() {
        first.last_opened_at = crate::time::now_utc();
    }
    save_recent_raw(db, &recents)
}

fn load_recent_raw(db: &crate::database::Db) -> AppResult<Vec<RecentProject>> {
    match db.get_setting(SETTING_RECENT_FOLDERS)? {
        Some(json) => serde_json::from_str(&json).map_err(|e| {
            crate::error::AppError::Database(format!("recent_folders corrupt: {e}"))
        }),
        None => Ok(Vec::new()),
    }
}

fn save_recent_raw(db: &crate::database::Db, recents: &[RecentProject]) -> AppResult<()> {
    let json = serde_json::to_string(recents)
        .map_err(|e| crate::error::AppError::Database(e.to_string()))?;
    db.set_setting(SETTING_RECENT_FOLDERS, &json)
}

/// Seed a legacy single-folder install into recent_folders on first read.
fn ensure_recents_seeded(db: &crate::database::Db) -> AppResult<Vec<RecentProject>> {
    let mut recents = load_recent_raw(db)?;
    if recents.is_empty() {
        if let Some(active) = db.get_setting(SETTING_ACTIVE_FOLDER)? {
            if Path::new(&active).is_dir() {
                recents.push(recent_entry(&active, 0));
                let _ = save_recent_raw(db, &recents);
            }
        }
    }
    // prune entries whose folders no longer exist
    let before = recents.len();
    recents.retain(|r| Path::new(&r.path).is_dir());
    if recents.len() != before {
        let _ = save_recent_raw(db, &recents);
    }
    Ok(recents)
}

#[tauri::command]
pub fn get_recent_projects(state: State<AppState>) -> AppResult<Vec<RecentProject>> {
    ensure_recents_seeded(&state.db)
}

#[tauri::command]
pub fn remove_recent_project(state: State<AppState>, path: String) -> AppResult<()> {
    let mut recents = load_recent_raw(&state.db)?;
    recents.retain(|r| r.path != path);
    save_recent_raw(&state.db, &recents)
}

#[tauri::command]
pub fn clear_recent_projects(state: State<AppState>) -> AppResult<()> {
    state.db.clear_setting(SETTING_RECENT_FOLDERS)
}

#[tauri::command]
pub fn close_project(state: State<AppState>) -> AppResult<()> {
    state.db.clear_setting(SETTING_ACTIVE_FOLDER)?;
    // Ephemeral similarity groups belong to the previous project — clear them
    // so the next project starts clean; they regenerate lazily on "Find similar".
    let _ = state.db.clear_similarity_groups();
    Ok(())
}

/// Open a project: validate the folder, set it active, and record it in recents.
#[tauri::command]
pub fn open_project(state: State<AppState>, path: String) -> AppResult<()> {
    let p = Path::new(&path);
    if !p.is_dir() {
        return Err(crate::error::AppError::validation(format!(
            "Folder does not exist: {path}"
        )));
    }
    let canonical = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let s = canonical.display().to_string();
    state.db.set_setting(SETTING_ACTIVE_FOLDER, &s)?;
    touch_recent(&state.db, &s)?;

    // Per-catalog scaffolding (Sprint 19): ensure catalogs/<slug>.sqlite exists
    // so the directory is visibly populated. Active-DB hot-swap lands on
    // Sprint 20; for now the app stays on the single catalog.
    let slug: String = Path::new(&canonical)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".to_string())
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let catalog_path = state.paths.catalogs_dir().join(format!("{slug}.sqlite"));
    if !catalog_path.exists() {
        if let Ok(db) = crate::database::Db::open(&catalog_path) {
            let _ = db.migrate();
        }
    }
    // Previous project's ephemeral groups must not leak into the new one.
    let _ = state.db.clear_similarity_groups();
    Ok(())
}

#[tauri::command]
pub fn get_dashboard_layout(state: State<AppState>) -> AppResult<Option<DashboardLayout>> {
    match state.db.get_setting(SETTING_DASHBOARD_LAYOUT)? {
        Some(json) => {
            let v: DashboardLayout = serde_json::from_str(&json).map_err(|e| {
                crate::error::AppError::Database(format!("dashboard_layout corrupt: {e}"))
            })?;
            Ok(Some(v))
        }
        None => Ok(None),
    }
}

#[tauri::command]
pub fn set_dashboard_layout(state: State<AppState>, layout: DashboardLayout) -> AppResult<()> {
    let json = serde_json::to_string(&layout)
        .map_err(|e| crate::error::AppError::Database(e.to_string()))?;
    state.db.set_setting(SETTING_DASHBOARD_LAYOUT, &json)
}

#[tauri::command]
pub fn open_in_file_manager(path: String) -> AppResult<()> {
    let p = Path::new(&path);
    if !p.exists() {
        return Err(crate::error::AppError::validation(format!(
            "Path does not exist: {path}"
        )));
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| crate::error::AppError::operation(format!("Could not open file manager: {e}")))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| crate::error::AppError::operation(format!("Could not open Finder: {e}")))?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| crate::error::AppError::operation(format!("Could not open Explorer: {e}")))?;
    }
    Ok(())
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
