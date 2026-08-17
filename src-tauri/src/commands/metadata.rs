//! Metadata (EXIF) commands: start/stop the background metadata pass.
//!
//! `start_metadata` claims the single metadata slot, spawns the pipeline off
//! the UI thread and returns immediately. Progress and the final summary
//! stream through IPC events (`metadata-progress`, `metadata-complete`) so
//! the webview never blocks.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::Emitter;
use tauri::{AppHandle, State};

use crate::error::{AppError, AppResult};
use crate::events;
use crate::metadata::{self, MetadataSummary};
use crate::state::{AppState, Job};

/// Payload for the `metadata-complete` event: exactly one of the two is set.
#[derive(Debug, serde::Serialize)]
pub struct MetadataCompletePayload {
    pub summary: Option<MetadataSummary>,
    pub error: Option<String>,
}

/// Begin reading camera metadata from the local library in the background.
/// Rejects if a metadata pass is already running (scan/analysis slots are
/// independent; the UI keeps the passes sequential to stay responsive).
#[tauri::command]
pub async fn start_metadata(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<()> {
    // Claim the single metadata slot.
    let job = {
        let mut slot = state
            .metadata
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        if let Some(existing) = slot.as_ref() {
            if existing.running.load(Ordering::Relaxed) {
                return Err(AppError::operation("Metadata reading is already in progress"));
            }
        }
        let job = Arc::new(Job::new());
        *slot = Some(job.clone());
        job
    };

    let db = state.db.clone();
    let slot = state.metadata.clone();
    let cancel = job.cancel.clone();
    let running = job.running.clone();
    // Arc clones: the task consumes its own; the originals stay for the
    // post-run slot ownership check (pointer comparison) and final emit.
    let cancel_task = cancel.clone();
    let app_task = app.clone();

    tauri::async_runtime::spawn(async move {
        let result: AppResult<MetadataSummary> =
            match tauri::async_runtime::spawn_blocking(move || {
                metadata::run_metadata(
                    db,
                    Arc::new(move |p| {
                        let _ = app_task.emit(events::METADATA_PROGRESS, &p);
                    }),
                    cancel_task,
                )
            })
            .await
            {
                Ok(inner) => inner,
                Err(e) => Err(AppError::operation(format!("metadata task failed: {e}"))),
            };

        // Release the slot (only if we're still the holder) and tell the UI.
        {
            let mut slot = slot.lock().expect("metadata slot poisoned");
            if let Some(holder) = slot.as_ref() {
                if holder.cancel.as_ptr() == cancel.as_ptr() {
                    *slot = None;
                }
            }
        }
        running.store(false, Ordering::Relaxed);

        let payload = match result {
            Ok(summary) => MetadataCompletePayload {
                summary: Some(summary),
                error: None,
            },
            Err(e) => MetadataCompletePayload {
                summary: None,
                error: Some(e.to_string()),
            },
        };
        let _ = app.emit(events::METADATA_COMPLETE, &payload);
        tracing::info!(?payload.error, "metadata command finished");
    });

    Ok(())
}

/// Request cancellation of the running metadata pass (takes effect at the
/// next file). Returns whether a running pass was cancelled.
#[tauri::command]
pub fn stop_metadata(state: State<'_, AppState>) -> AppResult<bool> {
    let slot = state
        .metadata
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;
    if let Some(job) = slot.as_ref() {
        if job.running.load(Ordering::Relaxed) {
            job.cancel.store(true, Ordering::Relaxed);
            tracing::info!("metadata cancellation requested");
            return Ok(true);
        }
    }
    Ok(false)
}
