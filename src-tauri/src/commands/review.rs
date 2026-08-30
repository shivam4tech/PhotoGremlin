//! Session-first review data (Sprint 19a).
//!
//! Review is intentionally a read-only local queue: existing selection marks
//! remain the only persistent decision state, and no image is hidden or
//! deleted by this command.

use tauri::State;

use crate::database::{ReviewProgress, ReviewQueue};
use crate::error::AppResult;
use crate::state::AppState;

#[tauri::command]
pub fn review_queue(session_id: i64, state: State<'_, AppState>) -> AppResult<ReviewQueue> {
    state.db()?.review_queue(session_id)
}

#[tauri::command]
pub fn get_review_progress(
    session_id: i64,
    state: State<'_, AppState>,
) -> AppResult<Option<ReviewProgress>> {
    state.db()?.get_review_progress(session_id)
}

#[tauri::command]
pub fn set_review_progress(
    session_id: i64,
    unit_index: i64,
    focused_photo_id: Option<i64>,
    state: State<'_, AppState>,
) -> AppResult<()> {
    state
        .db()?
        .set_review_progress(session_id, unit_index, focused_photo_id)
}
