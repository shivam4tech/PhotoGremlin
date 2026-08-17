//! Scan commands: start/stop a background folder scan + list sessions.
//!
//! `start_scan` claims a single scan slot, spawns the scanner off the UI
//! thread and returns immediately. Progress and the final summary stream
//! through IPC events (`scan-progress`, `scan-complete`) so the webview
//! never blocks.

use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::Emitter;
use tauri::{AppHandle, State};

use crate::database::SessionRow;
use crate::error::{AppError, AppResult};
use crate::events;
use crate::scanner::{self, ScanSummary};
use crate::state::{AppState, Job};

/// Payload for the `scan-complete` event: exactly one of the two is set.
#[derive(Debug, serde::Serialize)]
pub struct ScanCompletePayload {
    pub summary: Option<ScanSummary>,
    pub error: Option<String>,
}

/// Begin scanning `path` in the background. Rejects if a scan is running.
#[tauri::command]
pub async fn start_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> AppResult<()> {
    let root = Path::new(&path);
    if !root.is_dir() {
        return Err(AppError::validation(format!("Folder does not exist: {path}")));
    }
    let canonical = root
        .canonicalize()
        .map_err(|e| AppError::io(e, path.clone()))?;

    // Claim the single scan slot.
    let job = {
        let mut slot = state
            .scan
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        if let Some(existing) = slot.as_ref() {
            if existing.running.load(Ordering::Relaxed) {
                return Err(AppError::operation("A scan is already in progress"));
            }
        }
        let job = Arc::new(Job::new());
        *slot = Some(job.clone());
        job
    };

    let db = state.db.clone();
    let scan_slot = state.scan.clone();
    let cancel = job.cancel.clone();
    let running = job.running.clone();
    // Arc clones: the task consumes its own; the originals stay for the
    // post-scan slot ownership check (pointer comparison) and final emit.
    let cancel_task = cancel.clone();
    let app_task = app.clone();

    tauri::async_runtime::spawn(async move {
        let result: AppResult<ScanSummary> = match tauri::async_runtime::spawn_blocking(move || {
            scanner::run_scan(&canonical, &db, &mut |p| {
                let _ = app_task.emit(events::SCAN_PROGRESS, &p);
            }, &cancel_task)
        })
        .await
        {
            Ok(inner) => inner,
            Err(e) => Err(AppError::operation(format!("scan task failed: {e}"))),
        };

        // Release the slot (only if we're still the holder) and tell the UI.
        {
            let mut slot = scan_slot.lock().expect("scan slot poisoned");
            if let Some(holder) = slot.as_ref() {
                if holder.cancel.as_ptr() == cancel.as_ptr() {
                    *slot = None;
                }
            }
        }
        running.store(false, Ordering::Relaxed);

        let payload = match result {
            Ok(summary) => ScanCompletePayload {
                summary: Some(summary),
                error: None,
            },
            Err(e) => ScanCompletePayload {
                summary: None,
                error: Some(e.to_string()),
            },
        };
        let _ = app.emit(events::SCAN_COMPLETE, &payload);
        tracing::info!(?payload.error, "scan command finished");
    });

    Ok(())
}

/// Request cancellation of the running scan (takes effect at the next file).
/// Returns whether a running scan existed to cancel.
#[tauri::command]
pub fn stop_scan(state: State<'_, AppState>) -> AppResult<bool> {
    let slot = state
        .scan
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;
    if let Some(job) = slot.as_ref() {
        if job.running.load(Ordering::Relaxed) {
            job.cancel.store(true, Ordering::Relaxed);
            tracing::info!("scan cancellation requested");
            return Ok(true);
        }
    }
    Ok(false)
}

#[tauri::command]
pub fn list_sessions(state: State<'_, AppState>) -> AppResult<Vec<SessionRow>> {
    state.db.list_sessions()
}
