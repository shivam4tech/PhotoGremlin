//! Contact-sheet export command (Sprint 14).
//!
//! Mirrors the background-job shape of file operations: `export_contact_sheet`
//! claims the export slot, spawns a task that (1) fetches metadata + small
//! thumbnails (bounded by the thumbnail semaphore) and (2) renders PNG
//! pages on a blocking thread, streaming `contact-sheet-progress` and
//! finishing with `contact-sheet-complete`.

use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::Emitter;
use tauri::{AppHandle, State};

use crate::contact_sheet::{self, SheetOutcome};
use crate::error::{AppError, AppResult};
use crate::events::{self, ContactSheetCompletePayload, ProgressPayload};
use crate::state::{AppState, Job};

fn claim_export_slot(state: &State<'_, AppState>) -> AppResult<Arc<Job>> {
    let mut slot = state
        .export
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;
    if let Some(existing) = slot.as_ref() {
        if existing.running.load(Ordering::Relaxed) {
            return Err(AppError::operation("A contact-sheet export is already in progress"));
        }
    }
    let job = Arc::new(Job::new());
    *slot = Some(job.clone());
    Ok(job)
}

/// Render a printable contact sheet of the given photographs into
/// `dest_dir` (must already exist). Returns immediately; progress and the
/// written file paths arrive via the `contact-sheet-*` events.
#[tauri::command]
pub fn export_contact_sheet(
    app: AppHandle,
    state: State<'_, AppState>,
    dest_dir: String,
    title: String,
    photo_ids: Vec<i64>,
) -> AppResult<()> {
    let dest = Path::new(&dest_dir);
    if !dest.is_dir() {
        return Err(AppError::validation(format!(
            "The export folder does not exist: {dest_dir}"
        )));
    }
    // Fail fast on permissions before claiming the slot.
    let probe = dest.join(".pg-export-probe");
    if std::fs::write(&probe, b"probe").is_err() {
        return Err(AppError::validation(format!(
            "Cannot write into {dest_dir} — pick a folder you have write access to."
        )));
    }
    let _ = std::fs::remove_file(&probe);

    let job = claim_export_slot(&state)?;
    let db = state.db.clone();
    let thumb = state.thumb.clone();
    let slot = state.export.clone();
    let cancel = job.cancel.clone();
    let running = job.running.clone();
    let cancel_task = cancel.clone();
    let app_task = app.clone();
    let title = if title.trim().is_empty() {
        "Untitled contact sheet".to_string()
    } else {
        title
    };

    tauri::async_runtime::spawn(async move {
        let result: AppResult<(Vec<String>, bool)> = async {
            // Phase 1 (async): fetch names/dates + small thumbnails.
            let mut photos = contact_sheet::sheet_photos(&db, &photo_ids)?;
            for (i, p) in photos.iter_mut().enumerate() {
                if cancel_task.load(Ordering::Relaxed) {
                    return Ok((Vec::new(), true));
                }
                match thumb.get(&db, photo_ids[i], crate::thumbnailer::ThumbKind::Sheet).await {
                    Ok(t) => {
                        if let Some(bytes) = crate::thumbnailer::b64_decode(
                            t.data_url.trim_start_matches("data:image/jpeg;base64,"),
                        ) {
                            p.thumb = Some((bytes, t.width, t.height));
                        }
                    }
                    Err(_) => {} // placeholder box on the sheet
                }
                let _ = app_task.emit(
                    events::CONTACT_SHEET_PROGRESS,
                    &ProgressPayload::new(photo_ids.len(), i + 1, "loading")
                        .with_current(p.filename.clone()),
                );
            }

            // Phase 2 (blocking): composite + encode PNG pages.
            let mut render_progress =
                move |p: ProgressPayload| {
                    let _ = app_task.emit(events::CONTACT_SHEET_PROGRESS, &p);
                };
            let dest_dir = dest_dir.clone();
            let title = title.clone();
            let outcome = tauri::async_runtime::spawn_blocking(move || {
                contact_sheet::render_sheets(
                    &mut photos,
                    &title,
                    Path::new(&dest_dir),
                    &mut render_progress,
                    &cancel_task,
                )
            })
            .await
            .map_err(|e| AppError::operation(format!("contact-sheet task crashed: {e}")))??;

            match outcome {
                SheetOutcome::Ok { pages } => Ok((
                    pages.into_iter().map(|p| p.display().to_string()).collect(),
                    false,
                )),
                SheetOutcome::Cancelled { pages } => Ok((
                    pages.into_iter().map(|p| p.display().to_string()).collect(),
                    true,
                )),
            }
        }
        .await;

        {
            let mut slot = slot.lock().expect("export slot poisoned");
            if let Some(holder) = slot.as_ref() {
                if holder.cancel.as_ptr() == cancel.as_ptr() {
                    *slot = None;
                }
            }
        }
        running.store(false, Ordering::Relaxed);

        let payload = match result {
            Ok((files, cancelled)) => ContactSheetCompletePayload {
                files,
                error: None,
                cancelled,
            },
            Err(e) => ContactSheetCompletePayload {
                files: Vec::new(),
                error: Some(e.to_string()),
                cancelled: false,
            },
        };
        let _ = app.emit(events::CONTACT_SHEET_COMPLETE, &payload);
        tracing::info!(error = ?payload.error, files = payload.files.len(), "contact sheet export finished");
    });

    Ok(())
}

/// Stop the running export (checked between pages; already-written pages
/// stay on disk and are reported in the completion event).
#[tauri::command]
pub fn stop_export(state: State<'_, AppState>) -> AppResult<bool> {
    let slot = state
        .export
        .lock()
        .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(match slot.as_ref() {
        Some(job) => {
            let was_running = job.running.load(Ordering::Relaxed);
            job.cancel.store(true, Ordering::Relaxed);
            was_running
        }
        None => false,
    })
}