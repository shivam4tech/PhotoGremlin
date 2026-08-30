//! File-operation commands (Sprint 7): preview (plan) + execute (start) for
//! group rename, move/copy and trash, selection state, and the audit log.
//!
//! Plans are synchronous and cheap (stat-based) so the preview is instant;
//! execution runs in the single operation slot off the UI thread and streams
//! `operation-progress` / `operation-complete` events.

use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::Emitter;
use tauri::{AppHandle, State};

use crate::database::{FileOpRow, SelectionPage};
use crate::error::{AppError, AppResult};
use crate::events;
use crate::filesystem::{
    self, CollisionPolicy, FileOpPlan, OperationSummary, OpKind,
};
use crate::state::{AppState, Job};

/// Payload for the `operation-complete` event: exactly one of the two is set.
#[derive(Debug, serde::Serialize)]
pub struct OperationCompletePayload {
    pub summary: Option<OperationSummary>,
    pub error: Option<String>,
}

fn claim_operation_slot(state: &State<'_, AppState>) -> AppResult<Arc<Job>> {
    let mut slot = state
        .operation
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;
    if let Some(existing) = slot.as_ref() {
        if existing.running.load(Ordering::Relaxed) {
            return Err(AppError::operation("A file operation is already in progress"));
        }
    }
    let job = Arc::new(Job::new());
    *slot = Some(job.clone());
    Ok(job)
}

fn spawn_operation(
    app: AppHandle,
    state: &State<'_, AppState>,
    op: &'static str,
    plan: FileOpPlan,
) -> AppResult<()> {
    let job = claim_operation_slot(state)?;

    let db = state.db()?;
    let op_slot = state.operation.clone();
    let cancel = job.cancel.clone();
    let running = job.running.clone();
    let cancel_task = cancel.clone();
    let app_task = app.clone();

    tauri::async_runtime::spawn(async move {
        let result: AppResult<OperationSummary> =
            match tauri::async_runtime::spawn_blocking(move || {
                let mut progress = |p: crate::events::ProgressPayload| {
                    let _ = app_task.emit(events::OPERATION_PROGRESS, &p);
                };
                filesystem::run_operation(&db, &plan, &mut progress, &cancel_task)
            })
            .await
            {
                Ok(inner) => inner,
                Err(e) => Err(AppError::operation(format!("{op} task failed: {e}"))),
            };

        {
            let mut slot = op_slot.lock().expect("operation slot poisoned");
            if let Some(holder) = slot.as_ref() {
                if holder.cancel.as_ptr() == cancel.as_ptr() {
                    *slot = None;
                }
            }
        }
        running.store(false, Ordering::Relaxed);

        let payload = match result {
            Ok(summary) => OperationCompletePayload {
                summary: Some(summary),
                error: None,
            },
            Err(e) => OperationCompletePayload {
                summary: None,
                error: Some(e.to_string()),
            },
        };
        let _ = app.emit(events::OPERATION_COMPLETE, &payload);
        tracing::info!(op, error = ?payload.error, "file operation finished");
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// Plan commands (synchronous previews)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn plan_group_rename(
    photo_ids: Vec<i64>,
    template: String,
    group_name: String,
    state: State<AppState>,
) -> AppResult<FileOpPlan> {
    let db = state.db()?;
    filesystem::plan_rename(&db, &photo_ids, &template, &group_name)
}

#[tauri::command]
pub fn plan_move_copy(
    photo_ids: Vec<i64>,
    dest_dir: String,
    op: String,
    on_collision: String,
    state: State<AppState>,
) -> AppResult<FileOpPlan> {
    let kind = OpKind::parse(&op)?;
    let policy = CollisionPolicy::parse(&on_collision)?;
    let db = state.db()?;
    filesystem::plan_move_copy(&db, &photo_ids, Path::new(&dest_dir), kind, policy)
}

#[tauri::command]
pub fn plan_trash(photo_ids: Vec<i64>, state: State<AppState>) -> AppResult<FileOpPlan> {
    let db = state.db()?;
    filesystem::plan_trash(&db, &photo_ids)
}

// ---------------------------------------------------------------------------
// Execute commands (background, event-driven)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn start_group_rename(
    app: AppHandle,
    state: State<'_, AppState>,
    photo_ids: Vec<i64>,
    template: String,
    group_name: String,
) -> AppResult<()> {
    let db = state.db()?;
    let plan = filesystem::plan_rename(&db, &photo_ids, &template, &group_name)?;
    if plan.aborted {
        return Err(AppError::validation(
            "Rename plan aborted: two or more photographs map to the same name".to_string(),
        ));
    }
    if plan.items.iter().all(|i| !i.ok) {
        return Err(AppError::validation("No photographs can be renamed (files no longer exist)".to_string()));
    }
    spawn_operation(app, &state, "rename", plan)
}

#[tauri::command]
pub fn start_move_copy(
    app: AppHandle,
    state: State<'_, AppState>,
    photo_ids: Vec<i64>,
    dest_dir: String,
    op: String,
    on_collision: String,
) -> AppResult<()> {
    let kind = OpKind::parse(&op)?;
    let policy = CollisionPolicy::parse(&on_collision)?;
    let db = state.db()?;
    let plan = filesystem::plan_move_copy(&db, &photo_ids, Path::new(&dest_dir), kind, policy)?;
    if plan.items.iter().all(|i| !i.ok) {
        return Err(AppError::validation(
            "No photographs can be moved or copied (destination collisions or missing files)"
                .to_string(),
        ));
    }
    spawn_operation(app, &state, kind.tag(), plan)
}

#[tauri::command]
pub fn start_trash(app: AppHandle, state: State<'_, AppState>, photo_ids: Vec<i64>) -> AppResult<()> {
    let db = state.db()?;
    let plan = filesystem::plan_trash(&db, &photo_ids)?;
    if plan.items.iter().all(|i| !i.ok) {
        return Err(AppError::validation("No photographs can be trashed (files no longer exist)".to_string()));
    }
    spawn_operation(app, &state, "trash", plan)
}

/// Request cancellation (takes effect between items). Returns whether a
/// running operation existed.
#[tauri::command]
pub fn stop_operation(state: State<AppState>) -> AppResult<bool> {
    let slot = state
        .operation
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;
    if let Some(job) = slot.as_ref() {
        if job.running.load(Ordering::Relaxed) {
            job.cancel.store(true, Ordering::Relaxed);
            tracing::info!("file operation cancellation requested");
            return Ok(true);
        }
    }
    Ok(false)
}

// ---------------------------------------------------------------------------
// Selection state + audit log
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn set_selection(photo_id: i64, state: State<AppState>, selection: String) -> AppResult<()> {
    state.db()?.set_selection(photo_id, &selection)
}

#[tauri::command]
pub fn set_selections(
    photo_ids: Vec<i64>,
    state: State<AppState>,
    selection: String,
) -> AppResult<usize> {
    state.db()?.set_selections(photo_ids, &selection)
}

#[tauri::command]
pub fn clear_selection(photo_id: i64, state: State<AppState>) -> AppResult<()> {
    state.db()?.clear_selection(photo_id)
}

#[tauri::command]
pub fn clear_selections(photo_ids: Vec<i64>, state: State<AppState>) -> AppResult<usize> {
    state.db()?.clear_selections(photo_ids)
}

#[tauri::command]
pub fn list_selections(
    state: State<AppState>,
    session_id: Option<i64>,
    after_photo_id: Option<i64>,
    limit: Option<i64>,
) -> AppResult<SelectionPage> {
    state.db()?.list_selections_page(
        session_id,
        after_photo_id.unwrap_or(0),
        limit.unwrap_or(2_000),
    )
}

/// Apply curatorial marks (rating / flag / color label) to a batch of
/// photos (Sprint 13). Only `Some` fields change; `None` leaves that mark
/// untouched on every photo. Rating 0 / empty color clear the mark.
#[tauri::command]
pub fn update_marks(
    photo_ids: Vec<i64>,
    state: State<AppState>,
    rating: Option<i64>,
    flag: Option<bool>,
    color: Option<String>,
) -> AppResult<usize> {
    state
        .db()?
        .set_marks(&photo_ids, rating, flag, color.as_deref())
}

#[tauri::command]
pub fn recent_file_ops(state: State<AppState>, limit: i64) -> AppResult<Vec<FileOpRow>> {
    state.db()?.recent_file_ops(limit)
}
