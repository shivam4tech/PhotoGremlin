//! Analysis commands: start/stop the background analysis pass.
//!
//! `start_analysis` claims the single analysis slot, spawns the pipeline
//! off the UI thread and returns immediately. Progress and the final summary
//! stream through IPC events (`analysis-progress`, `analysis-complete`) so
//! the webview never blocks.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::Emitter;
use tauri::{AppHandle, State};

use crate::analysis::{self, AnalysisSummary};
use crate::error::{AppError, AppResult};
use crate::events;
use crate::state::{AppState, Job};

/// Payload for the `analysis-complete` event: exactly one of the two is set.
#[derive(Debug, serde::Serialize)]
pub struct AnalysisCompletePayload {
    pub summary: Option<AnalysisSummary>,
    pub error: Option<String>,
}

/// Begin analyzing the local library in the background. Rejects if an
/// analysis pass is already running (the scan slot is separate; the UI
/// keeps the two exclusive while they would fight for CPU).
#[tauri::command]
pub async fn start_analysis(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<()> {
    // Claim the single analysis slot.
    let job = {
        let mut slot = state
            .analysis
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        if let Some(existing) = slot.as_ref() {
            if existing.running.load(Ordering::Relaxed) {
                return Err(AppError::operation("Analysis is already in progress"));
            }
        }
        let job = Arc::new(Job::new());
        *slot = Some(job.clone());
        job
    };

    let db = state.db()?;
    let slot = state.analysis.clone();
    let cancel = job.cancel.clone();
    let running = job.running.clone();
    // Arc clones: the task consumes its own; the originals stay for the
    // post-run slot ownership check (pointer comparison) and final emit.
    let cancel_task = cancel.clone();
    let app_task = app.clone();

    tauri::async_runtime::spawn(async move {
        let result: AppResult<AnalysisSummary> =
            match tauri::async_runtime::spawn_blocking(move || {
                analysis::run_analysis(
                    db,
                    Arc::new(move |p| {
                        let _ = app_task.emit(events::ANALYSIS_PROGRESS, &p);
                    }),
                    cancel_task,
                )
            })
            .await
            {
                Ok(inner) => inner,
                Err(e) => Err(AppError::operation(format!("analysis task failed: {e}"))),
            };

        // Release the slot (only if we're still the holder) and tell the UI.
        {
            let mut slot = slot.lock().expect("analysis slot poisoned");
            if let Some(holder) = slot.as_ref() {
                if holder.cancel.as_ptr() == cancel.as_ptr() {
                    *slot = None;
                }
            }
        }
        running.store(false, Ordering::Relaxed);

        let payload = match result {
            Ok(summary) => AnalysisCompletePayload {
                summary: Some(summary),
                error: None,
            },
            Err(e) => AnalysisCompletePayload {
                summary: None,
                error: Some(e.to_string()),
            },
        };
        let _ = app.emit(events::ANALYSIS_COMPLETE, &payload);
        tracing::info!(?payload.error, "analysis command finished");
    });

    Ok(())
}

/// Request cancellation of the running analysis pass (takes effect at the
/// next file). Returns whether a running pass was cancelled.
#[tauri::command]
pub fn stop_analysis(state: State<'_, AppState>) -> AppResult<bool> {
    let slot = state
        .analysis
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;
    if let Some(job) = slot.as_ref() {
        if job.running.load(Ordering::Relaxed) {
            job.cancel.store(true, Ordering::Relaxed);
            tracing::info!("analysis cancellation requested");
            return Ok(true);
        }
    }
    Ok(false)
}
