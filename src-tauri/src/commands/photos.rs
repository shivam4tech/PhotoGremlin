//! Photo browsing commands: grid pagination, full photo details, thumbnails.

use tauri::State;

use crate::database::PhotoPage;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::thumbnailer::{ThumbData, ThumbKind};

fn parse_kind(kind: String) -> AppResult<ThumbKind> {
    match kind.as_str() {
        "grid" => Ok(ThumbKind::Grid),
        "viewer" => Ok(ThumbKind::Viewer),
        other => Err(AppError::validation(format!(
            "Unknown thumbnail kind: {other}"
        ))),
    }
}

/// Paginated photo list for the library grid.
#[tauri::command]
pub fn list_photos(
    state: State<'_, AppState>,
    offset: i64,
    limit: i64,
) -> AppResult<PhotoPage> {
    let (photos, total) = state.db.list_photos(offset, limit)?;
    Ok(PhotoPage { photos, total })
}

/// Full photo + analysis for the viewer.
#[tauri::command]
pub fn get_photo_full(state: State<'_, AppState>, id: i64) -> AppResult<crate::database::PhotoFull> {
    state.db.get_photo_full(id)
}

/// Thumbnail for one photo (grid or viewer size). Served from local cache
/// when available; generation is bounded and deduplicated server-side.
///
/// The `State` guard is not Send across await points, so we clone the Arcs
/// out, drop the guard, and only then await the async service.
#[tauri::command]
pub async fn get_thumbnail(
    state: State<'_, AppState>,
    photo_id: i64,
    kind: String,
) -> AppResult<ThumbData> {
    let kind = parse_kind(kind)?;
    let thumb = state.thumb.clone();
    let db = state.db.clone();
    drop(state);
    thumb.get(&db, photo_id, kind).await
}
