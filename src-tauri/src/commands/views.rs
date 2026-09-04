//! Saved-view commands (Sprint 8): named, dynamic filters.
//!
//! A saved view is a name plus a structured filter (FILTER_ENGINE.md). The
//! filter is validated with the same engine the grid uses, so a saved view
//! can never hold a filter the grid cannot evaluate. Applying a view is a
//! frontend action: it loads the filter_json into the library grid.

use tauri::State;

use crate::database::SavedView;
use crate::error::{AppError, AppResult};
use crate::filters;
use crate::state::AppState;

/// All saved views, alphabetical.
#[tauri::command]
pub fn list_saved_views(state: State<'_, AppState>) -> AppResult<Vec<SavedView>> {
    state.db()?.list_saved_views()
}

/// Create or overwrite a saved view by name. The filter is validated before
/// anything is written; an invalid filter is a friendly error.
#[tauri::command]
pub fn save_view(
    state: State<'_, AppState>,
    name: String,
    filter_json: String,
    description: Option<String>,
) -> AppResult<i64> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::validation("View name is empty"));
    }
    // Validate with the grid's own engine before persisting (same build path
    // the grid uses, so a saved view is always evaluable by the grid).
    let filter = filters::parse_filter(&filter_json)?;
    let _ = filters::build_where(&filter)?;
    state.db()?.upsert_saved_view(&name, &filter_json, description.as_deref())
}

#[tauri::command]
pub fn rename_saved_view(
    state: State<'_, AppState>,
    id: i64,
    name: String,
) -> AppResult<()> {
    state.db()?.rename_saved_view(id, &name)
}

#[tauri::command]
pub fn delete_saved_view(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    state.db()?.delete_saved_view(id)
}

/// How many photographs a saved view matches right now (dynamic — recomputes
/// against the current library, so the count stays honest as it changes).
#[tauri::command]
pub fn saved_view_count(state: State<'_, AppState>, id: i64) -> AppResult<i64> {
    let view = state
        .db()?
        .list_saved_views()?
        .into_iter()
        .find(|v| v.id == id)
        .ok_or_else(|| AppError::validation(format!("Saved view {id} not found")))?;
    let filter = filters::parse_filter(&view.filter_json)?;
    let (where_sql, params) = filters::build_where(&filter)?;
    let (_, total) = state.db()?.photos_where(&where_sql, params, 0, 1)?;
    Ok(total)
}
