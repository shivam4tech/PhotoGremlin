//! Metadata (EXIF) pass (Sprint 5).
//!
//! `queue → read (bounded workers) → per-file extraction → upsert → progress
//! event` — deterministic, local, privacy-preserving. `run_metadata` is
//! Tauri-free (`Arc<Db>`, a progress callback, a cancel flag) so the exact
//! pipeline that ships is integration-tested with synthetic EXIF files, the
//! same way `run_analysis` is.
//!
//! Design notes (DATABASE.md / PRIVACY.md):
//! - Every queued file is read once and stamped (`photos.exif_at`), so a
//!   re-run's queue is empty; files that change on disk are re-read in a
//!   future reconcile.
//! - A readable image with no EXIF segment is a normal outcome (empty
//!   record), not a failure.
//! - GPS: the catalog stores a presence bit only — coordinates never leave
//!   the file.
//!
//! Work distribution mirrors the analysis pass: the queue is de-interleaved
//! round-robin into one slice per worker (item i → worker i % N); no
//! channels, balanced count and average cost. Cancel is cooperative: each
//! worker re-checks the flag before every item.

pub mod estimate;
pub mod exif;

pub use estimate::{estimate_datetime, DateEstimate};
pub use exif::ExifRecord;

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

use crate::database::{Db, ExifWork};
use crate::error::{AppError, AppResult};
use crate::events::ProgressPayload;

/// Do not stream-parse files this big (a corrupt huge file would dominate a
/// pass). Such files are stamped processed with no data.
const MAX_METADATA_FILE_BYTES: u64 = 256 * 1024 * 1024;
/// Concurrent EXIF readers (sequential file I/O is the bottleneck; the small
/// fixed count keeps the library responsive — same policy as thumbnails).
pub const METADATA_WORKERS: usize = 3;
/// First few friendly per-file messages the summary carries (the log has all).
const MAX_REPORTED_ERRORS: usize = 20;

/// Cooperative pause gate for a metadata pass. It never holds a database
/// lock while waiting, and cancellation wakes every paused worker promptly.
#[derive(Clone)]
pub struct PauseControl {
    paused: Arc<AtomicBool>,
    wake: Arc<(Mutex<()>, Condvar)>,
}

impl PauseControl {
    pub fn new() -> Self {
        Self {
            paused: Arc::new(AtomicBool::new(false)),
            wake: Arc::new((Mutex::new(()), Condvar::new())),
        }
    }

    pub fn pause(&self) {
        self.paused.store(true, Ordering::Relaxed);
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::Relaxed);
        self.wake.1.notify_all();
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    /// Wait only between files, allowing an in-progress EXIF read to finish
    /// safely. Returns false when cancellation was requested.
    fn wait_if_paused(&self, cancel: &AtomicBool) -> bool {
        if !self.is_paused() {
            return !cancel.load(Ordering::Relaxed);
        }
        let (lock, signal) = &*self.wake;
        let mut guard = lock.lock().expect("metadata pause lock poisoned");
        while self.is_paused() && !cancel.load(Ordering::Relaxed) {
            let (next_guard, _) = signal
                .wait_timeout(guard, std::time::Duration::from_millis(200))
                .expect("metadata pause wait poisoned");
            guard = next_guard;
        }
        !cancel.load(Ordering::Relaxed)
    }
}

impl Default for PauseControl {
    fn default() -> Self {
        Self::new()
    }
}

/// Outcome of one metadata pass (carried in `metadata-complete`).
#[derive(Debug, Clone, serde::Serialize)]
pub struct MetadataSummary {
    /// Photos read by this run (including ones with no EXIF inside).
    pub processed: usize,
    /// Photos that failed to read/parse (missing, corrupt, unreadable).
    pub failed: usize,
    /// True when the user stopped the run before the queue drained.
    pub cancelled: bool,
    pub elapsed_ms: u64,
    /// First few friendly messages; the log holds the full detail.
    pub errors: Vec<String>,
}

/// Run the metadata pass over the queued photos.
///
/// `progress` is called from worker threads after each completed/failed
/// item, so it must be `Fn + Send + Sync` (the command layer wraps its event
/// emit in an `Arc`). Cancel semantics are the same as the analysis pass.
pub fn run_metadata(
    db: Arc<Db>,
    progress: Arc<dyn Fn(ProgressPayload) + Send + Sync>,
    cancel: Arc<AtomicBool>,
) -> AppResult<MetadataSummary> {
    run_metadata_controlled(db, progress, cancel, PauseControl::new())
}

/// The command layer supplies the shared pause control. The public
/// `run_metadata` convenience entry remains pause-free for deterministic
/// integration tests and other non-interactive callers.
pub fn run_metadata_controlled(
    db: Arc<Db>,
    progress: Arc<dyn Fn(ProgressPayload) + Send + Sync>,
    cancel: Arc<AtomicBool>,
    pause: PauseControl,
) -> AppResult<MetadataSummary> {
    let queue = db.exif_queue()?;
    let total = queue.len();
    let start = Instant::now();

    if total == 0 {
        progress(ProgressPayload::new(0, 0, "reading metadata"));
        return Ok(MetadataSummary {
            processed: 0,
            failed: 0,
            cancelled: false,
            elapsed_ms: 0,
            errors: vec![],
        });
    }
    progress(ProgressPayload::new(total, 0, "reading metadata"));

    let done = Arc::new(AtomicUsize::new(0));
    let failed = Arc::new(AtomicUsize::new(0));
    let errors = Arc::new(Mutex::new(Vec::<String>::new()));

    // Round-robin de-interleave: worker i gets items i, i+N, i+2N, ....
    let n_workers = METADATA_WORKERS.min(total);
    let mut threads = Vec::new();
    for w in 0..n_workers {
        let slice: Vec<ExifWork> = queue
            .iter()
            .enumerate()
            .filter(|(i, _)| i % n_workers == w)
            .map(|(_, item)| item.clone())
            .collect();
        let db = db.clone();
        let progress = progress.clone();
        let cancel = cancel.clone();
        let pause = pause.clone();
        let done = done.clone();
        let failed = failed.clone();
        let errors = errors.clone();
        threads.push(std::thread::spawn(move || {
            for work in slice {
                if !pause.wait_if_paused(&cancel) {
                    break; // cooperative stop; remaining items drop
                }
                let filename = work.filename.clone();
                match process_one(&db, &work) {
                    Ok(()) => {
                        let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                        let f = failed.load(Ordering::Relaxed);
                        progress(
                            ProgressPayload::new(total, d + f, "reading metadata")
                                .with_current(filename),
                        );
                    }
                    Err(friendly) => {
                        failed.fetch_add(1, Ordering::Relaxed);
                        let d = done.load(Ordering::Relaxed);
                        {
                            let mut g = errors.lock().expect("errors mutex poisoned");
                            if g.len() < MAX_REPORTED_ERRORS {
                                g.push(friendly.clone());
                            }
                        }
                        progress(
                            ProgressPayload::new(total, d + 1, "reading metadata")
                                .with_current(filename),
                        );
                        tracing::warn!(path = %work.path, %friendly, "metadata item failed");
                    }
                }
            }
        }));
    }

    let mut panicked = 0usize;
    for t in threads {
        if t.join().is_err() {
            panicked += 1;
        }
    }
    if panicked > 0 {
        // Should be impossible (worker bodies only hit friendly-error paths);
        // surface it rather than reporting a clean summary.
        return Err(AppError::operation(format!(
            "{panicked} metadata worker thread(s) panicked"
        )));
    }

    let processed = done.load(Ordering::Relaxed);
    let nfailed = failed.load(Ordering::Relaxed);
    let cancelled = cancel.load(Ordering::Relaxed) && processed + nfailed < total;
    let errors = errors.lock().expect("errors mutex poisoned").clone();
    tracing::info!(total, processed, failed = nfailed, cancelled, "metadata pass finished");
    Ok(MetadataSummary {
        processed,
        failed: nfailed,
        cancelled,
        elapsed_ms: start.elapsed().as_millis() as u64,
        errors,
    })
}

/// Read one file's EXIF (plus capture-date estimation, Sprint 12) and store
/// it. Errors are friendly (shown to the user); details stay in the log.
fn process_one(db: &Db, work: &ExifWork) -> Result<(), String> {
    let path = Path::new(&work.path);
    if !path.exists() {
        return Err(format!("{} — file not found", work.filename));
    }
    let size = std::fs::metadata(path)
        .map(|m| m.len())
        .unwrap_or(0);
    if size > MAX_METADATA_FILE_BYTES {
        tracing::info!(path = %work.path, size, "file above metadata guard; skipping parse");
        // Still stamp it so the queue does not re-attempt forever.
        let estimate = estimate::estimate_datetime(&work.filename, work.file_mtime.as_deref());
        db.upsert_exif(work.photo_id, &exif::ExifRecord::default(), estimate.as_ref())
            .map_err(|e| {
                tracing::error!(path = %work.path, error = %e, "exif stamp failed");
                format!("{} — could not store the result", work.filename)
            })?;
        return Ok(());
    }
    let mut record = exif::extract_exif(path).map_err(|e| {
        let friendly = format!("{} — could not read metadata", work.filename);
        tracing::error!(path = %work.path, error = %e, %friendly, "exif read failed");
        friendly
    })?;
    // Derive orientation from the best-known dimensions: the scanner has
    // already resolved pixels for decodable files, EXIF fills the rest.
    let mw = record.width.or(work.width.and_then(|v| u32::try_from(v).ok()));
    let mh = record.height.or(work.height.and_then(|v| u32::try_from(v).ok()));
    record.orientation = match (mw, mh) {
        (Some(w), Some(h)) if w != 0 && h != 0 => Some(if w > h {
            "landscape".to_string()
        } else if w < h {
            "portrait".to_string()
        } else {
            "square".to_string()
        }),
        _ => None,
    };
    // Capture date: prefer EXIF; otherwise derive it (filename patterns
    // first, then the stored file mtime) so date filters and session
    // periods work on metadata-less files too. The estimate is labelled
    // per photo via `capture_datetime_source` (DATABASE.md).
    let estimate = if record.capture_datetime.is_none() {
        estimate::estimate_datetime(&work.filename, work.file_mtime.as_deref())
    } else {
        None
    };
    db.upsert_exif(work.photo_id, &record, estimate.as_ref()).map_err(|e| {
        tracing::error!(path = %work.path, error = %e, "exif upsert failed");
        format!("{} — could not store the result", work.filename)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_control_waits_and_resume_wakes_worker() {
        let control = PauseControl::new();
        control.pause();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_control = control.clone();
        let worker_cancel = cancel.clone();
        let (sent, received) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            sent.send(worker_control.wait_if_paused(&worker_cancel)).unwrap();
        });

        assert!(received.recv_timeout(std::time::Duration::from_millis(20)).is_err());
        control.resume();
        assert_eq!(received.recv_timeout(std::time::Duration::from_secs(1)).unwrap(), true);
        worker.join().unwrap();
    }
}
