//! Thumbnail-cache maintenance. Directory scans and eviction run off the UI
//! thread; only generated previews are touched, never source photographs.

use tauri::State;

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::thumbnailer::CacheStatus;

pub const SETTING_CACHE_QUOTA_BYTES: &str = "thumbnail_cache_quota_bytes";

#[tauri::command]
pub async fn cache_status(state: State<'_, AppState>) -> AppResult<CacheStatus> {
    let thumb = state.thumb.clone();
    tauri::async_runtime::spawn_blocking(move || thumb.status())
        .await
        .map_err(|error| AppError::operation(format!("Could not inspect the preview cache: {error}")))?
}

#[tauri::command]
pub async fn set_cache_quota(
    quota_bytes: u64,
    state: State<'_, AppState>,
) -> AppResult<CacheStatus> {
    let thumb = state.thumb.clone();
    let settings = state.settings_db.clone();
    let status = tauri::async_runtime::spawn_blocking(move || thumb.set_quota(quota_bytes))
        .await
        .map_err(|error| AppError::operation(format!("Could not resize the preview cache: {error}")))??;
    settings.set_setting(SETTING_CACHE_QUOTA_BYTES, &status.quota_bytes.to_string())?;
    Ok(status)
}

#[tauri::command]
pub async fn clear_cache(state: State<'_, AppState>) -> AppResult<CacheStatus> {
    let thumb = state.thumb.clone();
    tauri::async_runtime::spawn_blocking(move || thumb.clear())
        .await
        .map_err(|error| AppError::operation(format!("Could not clear the preview cache: {error}")))?
}
