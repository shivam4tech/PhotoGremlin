//! Statistics commands (Sprint 6). Synchronous — pure SQL aggregation
//! against the catalog; no background work, no events.

use chrono::Utc;
use tauri::State;

use crate::error::AppResult;
use crate::state::AppState;
use crate::statistics::{PeriodStats, SessionMetrics, SessionSummary};

/// Dashboard stats for a period. `period_json` is the one period model,
/// e.g. `{"kind":"this-month"}`; empty string means "all".
#[tauri::command]
pub fn period_stats(
    period_json: String,
    state: State<AppState>,
) -> AppResult<PeriodStats> {
    let period = crate::statistics::parse_period(&period_json)?;
    let db = state.db()?;
    crate::statistics::period_stats(&db, &period, Utc::now())
}

/// Full stats scoped to one session, plus shoot duration.
#[tauri::command]
pub fn session_summary(session_id: i64, state: State<AppState>) -> AppResult<SessionSummary> {
    let db = state.db()?;
    crate::statistics::session_summary(&db, session_id)
}

/// N sessions (≤ 8) side by side on the same metric rows.
#[tauri::command]
pub fn compare_sessions(
    session_ids: Vec<i64>,
    state: State<AppState>,
) -> AppResult<Vec<SessionMetrics>> {
    let db = state.db()?;
    crate::statistics::compare_sessions(&db, session_ids)
}
