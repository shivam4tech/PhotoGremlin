//! Session-first review data (Sprint 19a).
//!
//! Review is intentionally a read-only local queue: existing selection marks
//! remain the only persistent decision state, and no image is hidden or
//! deleted by this command.

use tauri::State;

use crate::database::ReviewQueue;
use crate::error::AppResult;
use crate::state::AppState;

#[tauri::command]
pub fn review_queue(session_id: i64, state: State<'_, AppState>) -> AppResult<ReviewQueue> {
    state.db.review_queue(session_id)
}
