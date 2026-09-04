//! Similarity commands (Sprint 8): start/stop the background
//! hash+group pass, and read the resulting groups.
//!
//! The pass runs off the UI thread and streams `similarity-progress` /
//! `similarity-complete` events. Groups are the product of it: "similar
//! photograph" clusters (perceptual hash) and bursts (time clusters).

use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::Emitter;
use tauri::{AppHandle, State};

use crate::database::{GroupPhotoSort, PhotoPage, SimilarityGroup};
use crate::error::{AppError, AppResult};
use crate::events;
use crate::similarity::{self, SimilaritySummary};
use crate::state::{AppState, Job};

/// Payload for the `similarity-complete` event: exactly one is set.
#[derive(Debug, serde::Serialize)]
pub struct SimilarityCompletePayload {
    pub summary: Option<SimilaritySummary>,
    pub error: Option<String>,
}

/// Hash every decodable photo that needs it and rebuild the similar + burst
/// groups, in the background. Rejects if a similarity run is already in
/// flight.
#[tauri::command]
pub async fn start_similarity(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let job = {
        let mut slot = state
            .similarity
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        if let Some(existing) = slot.as_ref() {
            if existing.running.load(Ordering::Relaxed) {
                return Err(AppError::operation("Finding similar photos is already in progress"));
            }
        }
        let job = Arc::new(Job::new());
        *slot = Some(job.clone());
        job
    };

    let db = state.db()?;
    let slot = state.similarity.clone();
    let cancel = job.cancel.clone();
    let running = job.running.clone();
    let cancel_task = cancel.clone();
    let app_task = app.clone();

    tauri::async_runtime::spawn(async move {
        let result: AppResult<SimilaritySummary> =
            match tauri::async_runtime::spawn_blocking(move || {
                similarity::run_similarity(
                    db,
                    Arc::new(move |p| {
                        let _ = app_task.emit(events::SIMILARITY_PROGRESS, &p);
                    }),
                    cancel_task,
                )
            })
            .await
            {
                Ok(inner) => inner,
                Err(e) => Err(AppError::operation(format!("similarity task failed: {e}"))),
            };

        {
            let mut slot = slot.lock().expect("similarity slot poisoned");
            if let Some(holder) = slot.as_ref() {
                if holder.cancel.as_ptr() == cancel.as_ptr() {
                    *slot = None;
                }
            }
        }
        running.store(false, Ordering::Relaxed);

        let payload = match result {
            Ok(summary) => SimilarityCompletePayload {
                summary: Some(summary),
                error: None,
            },
            Err(e) => SimilarityCompletePayload {
                summary: None,
                error: Some(e.to_string()),
            },
        };
        let _ = app.emit(events::SIMILARITY_COMPLETE, &payload);
        tracing::info!(?payload.error, "similarity command finished");
    });

    Ok(())
}

/// Request cancellation of a running similarity pass (takes effect at the
/// next file; grouping still completes on what is hashed).
#[tauri::command]
pub fn stop_similarity(state: State<'_, AppState>) -> AppResult<bool> {
    let slot = state
        .similarity
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;
    if let Some(job) = slot.as_ref() {
        if job.running.load(Ordering::Relaxed) {
            job.cancel.store(true, Ordering::Relaxed);
            tracing::info!("similarity cancellation requested");
            return Ok(true);
        }
    }
    Ok(false)
}

/// All current similar + burst groups (newest first, cover strip included).
#[tauri::command]
pub fn list_similarity_groups(
    state: State<'_, AppState>,
    limit: i64,
) -> AppResult<Vec<SimilarityGroup>> {
    state.db()?.list_similarity_groups(limit)
}

/// One page of a group's photographs (same summary shape as the grid), so a
/// group opens in the same viewer path as any library photo.
#[tauri::command]
pub fn group_photos(
    state: State<'_, AppState>,
    group_id: i64,
    offset: i64,
    limit: i64,
    sort: Option<GroupPhotoSort>,
) -> AppResult<PhotoPage> {
    let (photos, total) = state
        .db()?
        .group_photos(group_id, offset, limit, sort.unwrap_or_default())?;
    Ok(PhotoPage { photos, total })
}
