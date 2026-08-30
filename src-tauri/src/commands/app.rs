//! App-level commands: info, paths, database status, active library folder,
//! recent projects, project lifecycle, and dashboard layout.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::database::{Db, DbStatus, CURRENT_SCHEMA_VERSION};
use crate::error::{AppError, AppResult};
use crate::paths::AppPaths;
use crate::state::AppState;
use std::sync::Arc;

const SETTING_ACTIVE_FOLDER: &str = "active_folder";
const SETTING_RECENT_FOLDERS: &str = "recent_folders";
const SETTING_RECENT_FOLDERS_CAP: usize = 12;
const SETTING_DASHBOARD_LAYOUT: &str = "dashboard_layout";
const SETTING_CATALOG_PREFIX: &str = "project_catalog:";

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
        db_path: state.catalog_path().unwrap_or_else(|_| p.db_path()),
        thumbnails_dir: p.thumbnails_dir(),
    }
}

#[tauri::command]
pub fn db_status(state: State<AppState>) -> AppResult<DbStatus> {
    state.db()?.status()
}

/// Best-effort browser crash reporting. The payload is written only to the
/// existing local tracing log; it is never transmitted or surfaced as a raw
/// stack trace in the UI.
#[tauri::command]
pub fn log_client_error(source: String, message: String, stack: Option<String>) {
    const MAX_LOG_FIELD: usize = 16_000;
    let source = truncate_log_field(&source, MAX_LOG_FIELD);
    let message = truncate_log_field(&message, MAX_LOG_FIELD);
    let stack = stack
        .as_deref()
        .map(|value| truncate_log_field(value, MAX_LOG_FIELD));
    tracing::error!(%source, %message, stack = ?stack, "unhandled frontend error");
}

fn truncate_log_field(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… [truncated]", &value[..end])
}

#[cfg(test)]
mod tests {
    use super::truncate_log_field;

    #[test]
    fn truncation_preserves_utf8_boundaries() {
        assert_eq!(truncate_log_field("abcédef", 4), "abc… [truncated]");
        assert_eq!(truncate_log_field("brief", 16), "brief");
    }
}

/// Persist the active library folder (a scan root) and record it in recents.
#[tauri::command]
pub fn set_active_folder(state: State<AppState>, path: String) -> AppResult<()> {
    open_project_inner(&state, &path)
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
    resolve_active_folder(&state.settings_db)
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
fn touch_recent(settings_db: &Db, catalog_db: &Db, path: &str) -> AppResult<()> {
    let mut recents = load_recent_raw(settings_db)?;
    // count photos for this path's session if possible
    let photo_count = catalog_db
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
    save_recent_raw(settings_db, &recents)
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
    ensure_recents_seeded(&state.settings_db)
}

#[tauri::command]
pub fn remove_recent_project(state: State<AppState>, path: String) -> AppResult<()> {
    let mut recents = load_recent_raw(&state.settings_db)?;
    recents.retain(|r| r.path != path);
    save_recent_raw(&state.settings_db, &recents)
}

#[tauri::command]
pub fn clear_recent_projects(state: State<AppState>) -> AppResult<()> {
    state.settings_db.clear_setting(SETTING_RECENT_FOLDERS)
}

#[tauri::command]
pub fn close_project(state: State<AppState>) -> AppResult<()> {
    state.ensure_jobs_idle()?;
    state.db()?.clear_setting(SETTING_ACTIVE_FOLDER)?;
    state.settings_db.clear_setting(SETTING_ACTIVE_FOLDER)?;
    Ok(())
}

/// Open a project: validate the folder, set it active, and record it in recents.
#[tauri::command]
pub fn open_project(state: State<AppState>, path: String) -> AppResult<()> {
    open_project_inner(&state, &path)
}

fn canonical_project(path: &str) -> AppResult<PathBuf> {
    let p = Path::new(path);
    if !p.is_dir() {
        return Err(AppError::validation(format!(
            "Folder does not exist: {path}"
        )));
    }
    p.canonicalize().map_err(|e| AppError::io(e, path.to_string()))
}

fn project_hash(path: &Path) -> u64 {
    crate::thumbnailer::fnv1a64(path.to_string_lossy().as_bytes())
}

fn project_slug(path: &Path) -> String {
    let name: String = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".to_string())
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let name = name.trim_matches('_');
    let name = if name.is_empty() { "project" } else { name };
    format!("{name}-{:016x}", project_hash(path))
}

fn catalog_setting_key(path: &Path) -> String {
    format!("{SETTING_CATALOG_PREFIX}{:016x}", project_hash(path))
}

fn backup_stamp() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn open_catalog_checked(path: &Path, paths: &AppPaths) -> AppResult<Arc<Db>> {
    let existed = path.exists();
    let db = Arc::new(Db::open(path)?);
    if existed {
        db.integrity_check()?;
        let version = db.schema_version()?;
        if version > 0 && version < CURRENT_SCHEMA_VERSION {
            let backup = paths.catalog_backups_dir().join(format!(
                "pre-migration-{}-{}.sqlite",
                path.file_stem().unwrap_or_default().to_string_lossy(),
                backup_stamp()
            ));
            db.backup_to(&backup)?;
            tracing::info!(catalog = %path.display(), backup = %backup.display(), from = version, to = CURRENT_SCHEMA_VERSION, "catalog backed up before migration");
        }
    }
    db.migrate()?;
    db.integrity_check()?;
    Ok(db)
}

/// Resolve the catalog used on startup. A legacy single-catalog install is
/// registered to its current project exactly once, preserving every prior
/// scan and decision; subsequent projects receive isolated catalog files.
pub fn initial_catalog(settings_db: &Arc<Db>, paths: &AppPaths) -> AppResult<(Arc<Db>, PathBuf)> {
    let Some(active) = resolve_active_folder(settings_db)? else {
        return Ok((settings_db.clone(), paths.db_path()));
    };
    let project = PathBuf::from(&active);
    let key = catalog_setting_key(&project);
    let catalog_path = match settings_db.get_setting(&key)? {
        Some(mapped) => PathBuf::from(mapped),
        None => {
            // The pre-Sprint-27 database is the authoritative catalog for the
            // project that was active during upgrade.
            let legacy = paths.db_path();
            settings_db.set_setting(&key, &legacy.to_string_lossy())?;
            legacy
        }
    };
    let catalog = if catalog_path == paths.db_path() {
        settings_db.clone()
    } else {
        open_catalog_checked(&catalog_path, paths)?
    };
    catalog.set_setting(SETTING_ACTIVE_FOLDER, &active)?;
    Ok((catalog, catalog_path))
}

fn open_project_inner(state: &AppState, path: &str) -> AppResult<()> {
    state.ensure_jobs_idle()?;
    let canonical = canonical_project(path)?;
    let canonical_text = canonical.to_string_lossy().into_owned();
    let key = catalog_setting_key(&canonical);
    let catalog_path = state
        .settings_db
        .get_setting(&key)?
        .map(PathBuf::from)
        .unwrap_or_else(|| state.paths.catalog_db_path(&project_slug(&canonical)));

    let catalog = if state.is_active_catalog(&catalog_path)? {
        state.db()?
    } else {
        open_catalog_checked(&catalog_path, &state.paths)?
    };
    catalog.set_setting(SETTING_ACTIVE_FOLDER, &canonical_text)?;

    // Persist registry/active state only after the replacement is fully
    // usable; then swap both Arc and path in one short critical section.
    state
        .settings_db
        .set_setting(&key, &catalog_path.to_string_lossy())?;
    state
        .settings_db
        .set_setting(SETTING_ACTIVE_FOLDER, &canonical_text)?;
    state.switch_catalog(catalog.clone(), catalog_path)?;
    touch_recent(&state.settings_db, &catalog, &canonical_text)?;
    Ok(())
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CatalogHealth {
    pub path: PathBuf,
    pub schema_version: i64,
    pub healthy: bool,
}

#[tauri::command]
pub fn catalog_health(state: State<AppState>) -> AppResult<CatalogHealth> {
    let db = state.db()?;
    db.integrity_check()?;
    Ok(CatalogHealth {
        path: state.catalog_path()?,
        schema_version: db.schema_version()?,
        healthy: true,
    })
}

#[tauri::command]
pub fn backup_catalog(state: State<AppState>) -> AppResult<PathBuf> {
    state.ensure_jobs_idle()?;
    let db = state.db()?;
    let stem = state
        .catalog_path()?
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let destination = state
        .paths
        .catalog_backups_dir()
        .join(format!("{stem}-{}.sqlite", backup_stamp()));
    db.backup_to(&destination)?;
    Ok(destination)
}

#[tauri::command]
pub fn list_catalog_backups(state: State<AppState>) -> AppResult<Vec<PathBuf>> {
    let mut backups = std::fs::read_dir(state.paths.catalog_backups_dir())
        .map_err(|e| AppError::io(e, "catalog backup folder"))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("sqlite"))
        .collect::<Vec<_>>();
    backups.sort_by(|left, right| right.file_name().cmp(&left.file_name()));
    backups.truncate(20);
    Ok(backups)
}

#[tauri::command]
pub fn restore_catalog(state: State<AppState>, backup_path: String) -> AppResult<()> {
    state.ensure_jobs_idle()?;
    let requested = PathBuf::from(&backup_path);
    let backup = requested
        .canonicalize()
        .map_err(|e| AppError::io(e, backup_path.clone()))?;
    let root = state
        .paths
        .catalog_backups_dir()
        .canonicalize()
        .map_err(|e| AppError::io(e, "catalog backup folder"))?;
    if !backup.starts_with(&root) || backup.extension().and_then(|e| e.to_str()) != Some("sqlite") {
        return Err(AppError::validation(
            "Choose a PhotoGremlin catalog backup from the catalog-backups folder.",
        ));
    }
    let restored = state.paths.catalogs_dir().join(format!(
        "restored-{}-{}.sqlite",
        backup.file_stem().unwrap_or_default().to_string_lossy(),
        backup_stamp()
    ));
    std::fs::copy(&backup, &restored)
        .map_err(|e| AppError::io(e, restored.display().to_string()))?;
    let db = open_catalog_checked(&restored, &state.paths)?;
    let active = resolve_active_folder(&state.settings_db)?.ok_or_else(|| {
        AppError::validation("Open a project before restoring its catalog.")
    })?;
    if db
        .get_setting(SETTING_ACTIVE_FOLDER)?
        .is_some_and(|folder| folder != active)
    {
        return Err(AppError::validation(
            "That backup belongs to a different project. Open that project before restoring it.",
        ));
    }
    state.settings_db.set_setting(
        &catalog_setting_key(Path::new(&active)),
        &restored.to_string_lossy(),
    )?;
    db.set_setting(SETTING_ACTIVE_FOLDER, &active)?;
    state.switch_catalog(db, restored)
}

#[tauri::command]
pub fn get_dashboard_layout(state: State<AppState>) -> AppResult<Option<DashboardLayout>> {
    match state.settings_db.get_setting(SETTING_DASHBOARD_LAYOUT)? {
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
    state.settings_db.set_setting(SETTING_DASHBOARD_LAYOUT, &json)
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
