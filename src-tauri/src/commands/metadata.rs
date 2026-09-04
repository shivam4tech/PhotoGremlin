//! Metadata (EXIF) commands: start/stop the background metadata pass.
//!
//! `start_metadata` claims the single metadata slot, spawns the pipeline off
//! the UI thread and returns immediately. Progress and the final summary
//! stream through IPC events (`metadata-progress`, `metadata-complete`) so
//! the webview never blocks.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

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

    let db = state.db()?;
    let slot = state.metadata.clone();
    let cancel = job.cancel.clone();
    let running = job.running.clone();
    // Arc clones: the task consumes its own; the originals stay for the
    // post-run slot ownership check (pointer comparison) and final emit.
    let cancel_task = cancel.clone();
    let pause_task = job.pause.clone();
    let app_task = app.clone();
    // Hundreds or thousands of per-file EXIF updates can otherwise queue in
    // the webview faster than React can paint them. Keep the visible progress
    // smooth (10 Hz) while the backend still processes every file.
    let last_progress_emit = Arc::new(Mutex::new(None::<Instant>));
    // A second handle for the short post-run session-time refresh.
    let db_for_refresh = db.clone();

    tauri::async_runtime::spawn(async move {
        let result: AppResult<MetadataSummary> =
            match tauri::async_runtime::spawn_blocking(move || {
                metadata::run_metadata_controlled(
                    db,
                    Arc::new(move |p| {
                        let should_emit = {
                            let mut last = last_progress_emit
                                .lock()
                                .expect("metadata progress throttle poisoned");
                            let is_boundary = p.done == 0 || p.done >= p.total;
                            if is_boundary || last.map(|at| at.elapsed() >= Duration::from_millis(100)).unwrap_or(true) {
                                *last = Some(Instant::now());
                                true
                            } else {
                                false
                            }
                        };
                        if should_emit {
                            let _ = app_task.emit(events::METADATA_PROGRESS, &p);
                        }
                    }),
                    cancel_task,
                    pause_task,
                )
            })
            .await
            {
                Ok(inner) => inner,
                Err(e) => Err(AppError::operation(format!("metadata task failed: {e}"))),
            };

        // The pass just filled capture datetimes — derive session shoot
        // periods from them so the sessions view and statistics stay true
        // (one short blocking lock; no awaits around it).
        if let Err(e) = db_for_refresh.refresh_all_sessions_times() {
            tracing::warn!(error = %e, "session time refresh failed");
        }

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
            job.pause.resume();
            tracing::info!("metadata cancellation requested");
            return Ok(true);
        }
    }
    Ok(false)
}

/// Pause after the metadata reader completes its current file. The job keeps
/// its slot, so a second read cannot start and contend for the same library.
#[tauri::command]
pub fn pause_metadata(state: State<'_, AppState>) -> AppResult<bool> {
    let slot = state
        .metadata
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;
    if let Some(job) = slot.as_ref() {
        if job.running.load(Ordering::Relaxed) && !job.cancel.load(Ordering::Relaxed) {
            job.pause.pause();
            tracing::info!("metadata pause requested");
            return Ok(true);
        }
    }
    Ok(false)
}

/// Continue a paused metadata reader from its next queued file.
#[tauri::command]
pub fn resume_metadata(state: State<'_, AppState>) -> AppResult<bool> {
    let slot = state
        .metadata
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;
    if let Some(job) = slot.as_ref() {
        if job.running.load(Ordering::Relaxed) && !job.cancel.load(Ordering::Relaxed) {
            job.pause.resume();
            tracing::info!("metadata resume requested");
            return Ok(true);
        }
    }
    Ok(false)
}
