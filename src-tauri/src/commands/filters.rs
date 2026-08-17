//! Filtered browsing commands (Sprint 5).
//!
//! The library grid, saved views, and (later) statistics all consume the
//! same structured filter (FILTER_ENGINE.md). This command is the grid's
//! entry point: parse + validate the filter, then page the matching photos.
//! An empty filter returns the unfiltered list (one code path for all).

use tauri::State;

use crate::database::PhotoPage;
use crate::error::AppResult;
use crate::filters;
use crate::state::AppState;

/// Paginated photo list matching a structured filter. `filter_json` is the
/// exact wire object (or `""` for "no filter"). Validation errors are
/// friendly; no user string ever reaches a column name or value position.
#[tauri::command]
pub fn list_filtered_photos(
    state: State<'_, AppState>,
    filter_json: String,
    offset: i64,
    limit: i64,
) -> AppResult<PhotoPage> {
    let filter = filters::parse_filter(&filter_json)?;
    let (where_sql, params) = filters::build_where(&filter)?;
    let (photos, total) = state.db.photos_where(&where_sql, params, offset, limit)?;
    Ok(PhotoPage { photos, total })
}
