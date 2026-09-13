//! Local-AI commands (Sprint 9): status, the on/off preference, and
//! start/stop of the background face-detection pass.
//!
//! Core rules: the app is fully useful with AI off; the pass runs
//! like the similarity pass (one slot, cooperative cancel, progress events);
//! a missing ONNX Runtime is a status, never an error state of the app.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::Emitter;
use tauri::{AppHandle, State};

use crate::database::DbStatus;
use crate::error::{AppError, AppResult};
use crate::events;
use crate::ml::{self, FaceSummary};
use crate::state::{AppState, Job};

/// The shipped local model, reported as-is for transparency.
const MODEL_NAME: &str = "YuNet 2023mar (OpenCV Zoo, Apache-2.0)";
const EYE_MODEL_NAME: &str = "OCEC-S eye state (PINTO0309, MIT)";
const DEFAULT_AI_ENABLED: bool = true;

fn ai_enabled_from_setting(value: Option<&str>) -> bool {
    match value {
        Some("true") => true,
        Some("false") => false,
        _ => DEFAULT_AI_ENABLED,
    }
}

/// One row of the Settings "Local intelligence" card.
#[derive(Debug, serde::Serialize)]
pub struct AiStatus {
    /// The stored preference (`ai_enabled`); AI is on by default.
    pub enabled: bool,
    /// `true` when the ONNX Runtime library loaded on this machine.
    pub runtime_available: bool,
    /// Friendly reason when `runtime_available` is false (None otherwise).
    pub runtime_note: Option<String>,
    pub model: String,
    pub model_bytes: usize,
    pub eye_model: String,
    pub eye_model_bytes: usize,
    /// Photos with a stored face result / photos in the library.
    pub faces_done: i64,
    pub eyes_done: i64,
    /// Scene model (Sprint 18): name, size, and progress line.
    pub scene_model: String,
    pub scene_model_bytes: usize,
    pub scenes_done: i64,
    pub photo_count: i64,
}

/// Local-AI status (cheap; called when Settings opens and after runs).
#[tauri::command]
pub fn ai_status(state: State<'_, AppState>) -> AppResult<AiStatus> {
    let stored_preference = state.settings_db.get_setting("ai_enabled")?;
    let enabled = ai_enabled_from_setting(stored_preference.as_deref());
    let runtime = ml::runtime_status();
    let runtime_available = runtime.is_ok();
    let runtime_note = runtime.err();
    let status: DbStatus = state.db()?.status()?;
    Ok(AiStatus {
        enabled,
        runtime_available,
        runtime_note,
        model: MODEL_NAME.to_string(),
        model_bytes: ml::model_bytes(),
        eye_model: EYE_MODEL_NAME.to_string(),
        eye_model_bytes: ml::eye_model_bytes(),
        faces_done: status.faces_done,
        eyes_done: status.eyes_done,
        scene_model: ml::scene::SCENE_MODEL_NAME.to_string(),
        scene_model_bytes: ml::scene::SCENE_MODEL.len(),
        scenes_done: status.scenes_done,
        photo_count: status.photo_count,
    })
}

/// Persist the AI on/off preference. The UI drains pending work after turning
/// it on; turning it off also cooperatively stops an in-flight face/eye pass.
#[tauri::command]
pub fn set_ai_enabled(state: State<'_, AppState>, enabled: bool) -> AppResult<()> {
    state
        .settings_db
        .set_setting("ai_enabled", if enabled { "true" } else { "false" })?;

    if !enabled {
        let slot = state
            .faces
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        if let Some(job) = slot.as_ref() {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }
    Ok(())
}

/// Payload for the `faces-complete` event: exactly one is set.
#[derive(Debug, serde::Serialize)]
pub struct FaceCompletePayload {
    pub summary: Option<FaceSummary>,
    pub error: Option<String>,
}

/// Run face detection over the queued photos, in the background. Rejects if
/// a face run is already in flight.
#[tauri::command]
pub async fn start_faces(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let stored_preference = state.settings_db.get_setting("ai_enabled")?;
    if !ai_enabled_from_setting(stored_preference.as_deref()) {
        return Err(AppError::operation(
            "Local intelligence is turned off. Turn it on in Settings before analyzing faces and eye state.",
        ));
    }

    let job = {
        let mut slot = state
            .faces
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        if let Some(existing) = slot.as_ref() {
            if existing.running.load(Ordering::Relaxed) {
                return Err(AppError::operation("Face detection is already in progress"));
            }
        }
        let job = Arc::new(Job::new());
        *slot = Some(job.clone());
        job
    };

    let db = state.db()?;
    let slot = state.faces.clone();
    let cancel = job.cancel.clone();
    let running = job.running.clone();
    let cancel_task = cancel.clone();
    let app_task = app.clone();

    tauri::async_runtime::spawn(async move {
        let result: AppResult<FaceSummary> =
            match tauri::async_runtime::spawn_blocking(move || {
                ml::run_faces_pass(
                    db,
                    Arc::new(move |p| {
                        let _ = app_task.emit(events::FACES_PROGRESS, &p);
                    }),
                    cancel_task,
                )
            })
            .await
            {
                Ok(inner) => inner,
                Err(e) => Err(AppError::operation(format!("face run task failed: {e}"))),
            };

        {
            let mut slot = slot.lock().expect("faces slot poisoned");
            if let Some(holder) = slot.as_ref() {
                if holder.cancel.as_ptr() == cancel.as_ptr() {
                    *slot = None;
                }
            }
        }
        running.store(false, Ordering::Relaxed);

        let payload = match result {
            Ok(summary) => FaceCompletePayload {
                summary: Some(summary),
                error: None,
            },
            Err(e) => FaceCompletePayload {
                summary: None,
                error: Some(e.to_string()),
            },
        };
        let _ = app.emit(events::FACES_COMPLETE, &payload);
        tracing::info!(?payload.error, "faces command finished");
    });

    Ok(())
}

/// Request cancellation of a running face pass (takes effect at the next
/// file; already-detected results are kept).
#[tauri::command]
pub fn stop_faces(state: State<'_, AppState>) -> AppResult<bool> {
    let slot = state
        .faces
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;
    if let Some(job) = slot.as_ref() {
        if job.running.load(Ordering::Relaxed) {
            job.cancel.store(true, Ordering::Relaxed);
            tracing::info!("faces cancellation requested");
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod preference_tests {
    use super::ai_enabled_from_setting;

    #[test]
    fn local_intelligence_defaults_on_without_a_stored_preference() {
        assert!(ai_enabled_from_setting(None));
        assert!(ai_enabled_from_setting(Some("true")));
    }

    #[test]
    fn an_explicit_opt_out_stays_off() {
        assert!(!ai_enabled_from_setting(Some("false")));
    }
}

/// Payload for the `scenes-complete` event: exactly one is set.
#[derive(Debug, serde::Serialize)]
pub struct SceneCompletePayload {
    pub summary: Option<ml::scene::SceneSummary>,
    pub error: Option<String>,
}

/// Run scene classification over the queued photos, in the background.
/// Same single-slot + cooperative-cancel rules as the face pass.
#[tauri::command]
pub async fn start_scene_classification(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let job = {
        let mut slot = state
            .scenes
            .lock()
            .map_err(|e| AppError::Database(e.to_string()))?;
        if let Some(existing) = slot.as_ref() {
            if existing.running.load(Ordering::Relaxed) {
                return Err(AppError::operation("Scene classification is already in progress"));
            }
        }
        let job = Arc::new(Job::new());
        *slot = Some(job.clone());
        job
    };

    let db = state.db()?;
    let slot = state.scenes.clone();
    let cancel = job.cancel.clone();
    let running = job.running.clone();
    let cancel_task = cancel.clone();
    let app_task = app.clone();

    tauri::async_runtime::spawn(async move {
        let result: AppResult<ml::scene::SceneSummary> =
            match tauri::async_runtime::spawn_blocking(move || {
                ml::scene::run_scenes_pass(
                    db,
                    Arc::new(move |p| {
                        let _ = app_task.emit(events::SCENES_PROGRESS, &p);
                    }),
                    cancel_task,
                )
            })
            .await
            {
                Ok(inner) => inner,
                Err(e) => Err(AppError::operation(format!("scene run task failed: {e}"))),
            };

        running.store(false, Ordering::Relaxed);

        {
            let mut slot = slot.lock().expect("scenes slot poisoned");
            if let Some(holder) = slot.as_ref() {
                if holder.cancel.as_ptr() == cancel.as_ptr() {
                    *slot = None;
                }
            }
        }

        let payload = match result {
            Ok(summary) => SceneCompletePayload {
                summary: Some(summary),
                error: None,
            },
            Err(e) => SceneCompletePayload {
                summary: None,
                error: Some(e.to_string()),
            },
        };
        let _ = app.emit(events::SCENES_COMPLETE, &payload);
        tracing::info!(?payload.error, "scenes command finished");
    });
    Ok(())
}

/// Request cancellation of a running scene pass (takes effect at the next
/// file; already-classified results are kept).
#[tauri::command]
pub fn stop_scene_classification(state: State<'_, AppState>) -> AppResult<bool> {
    let slot = state
        .scenes
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;
    if let Some(job) = slot.as_ref() {
        if job.running.load(Ordering::Relaxed) {
            job.cancel.store(true, Ordering::Relaxed);
            tracing::info!("scenes cancellation requested");
            return Ok(true);
        }
    }
    Ok(false)
}
