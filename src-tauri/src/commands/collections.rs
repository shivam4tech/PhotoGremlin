//! Collection commands (Sprint 8): manually curated sets of photographs.
//!
//! Collections never touch files — they are joins between photographs and a
//! named set. Deleting a collection removes only the membership, never the
//! photographs. All names are trimmed and uniqueness is enforced in the DB
//! (surfaces as a friendly validation error).

use tauri::State;

use crate::database::{Collection, PhotoPage};
use crate::error::AppResult;
use crate::state::AppState;

/// All collections with their current photo counts.
#[tauri::command]
pub fn list_collections(state: State<'_, AppState>) -> AppResult<Vec<Collection>> {
    state.db()?.list_collections()
}

#[tauri::command]
pub fn create_collection(
    state: State<'_, AppState>,
    name: String,
    description: Option<String>,
) -> AppResult<i64> {
    state.db()?.create_collection(&name, description.as_deref())
}

#[tauri::command]
pub fn rename_collection(
    state: State<'_, AppState>,
    id: i64,
    name: String,
) -> AppResult<()> {
    state.db()?.rename_collection(id, &name)
}

#[tauri::command]
pub fn delete_collection(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    state.db()?.delete_collection(id)
}

/// Add photographs to a collection (idempotent). Returns how many were newly
/// added.
#[tauri::command]
pub fn add_to_collection(
    state: State<'_, AppState>,
    collection_id: i64,
    photo_ids: Vec<i64>,
) -> AppResult<usize> {
    state.db()?.add_to_collection(collection_id, photo_ids)
}

/// Remove photographs from a collection. Returns how many were removed.
#[tauri::command]
pub fn remove_from_collection(
    state: State<'_, AppState>,
    collection_id: i64,
    photo_ids: Vec<i64>,
) -> AppResult<usize> {
    state.db()?.remove_from_collection(collection_id, photo_ids)
}

/// One page of a collection's photographs for the grid.
#[tauri::command]
pub fn collection_photos(
    state: State<'_, AppState>,
    collection_id: i64,
    offset: i64,
    limit: i64,
) -> AppResult<PhotoPage> {
    let (photos, total) = state.db()?.collection_photos(collection_id, offset, limit)?;
    Ok(PhotoPage { photos, total })
}
