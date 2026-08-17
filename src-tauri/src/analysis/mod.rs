//! Local image analysis pipeline (Sprint 4).
//!
//! `decode (bounded workers) → measure → store row (algorithm_version) →
//! progress event` — deterministic, local, AI-free (IMAGE_ANALYSIS.md).
//!
//! `run_analysis` is Tauri-free (`Arc<Db>`, a progress callback, a cancel
//! flag) so the exact pipeline that ships is integration-tested in
//! `tests/analysis_integration.rs` with synthetic images. Decoding is the
//! memory hotspot: at most `ANALYSIS_WORKERS` images are measured at once,
//! each at a bounded working resolution. Re-runs are incremental: a photo is
//! re-measured only if it has no row, its row is from an older algorithm, or
//! the file's mtime changed (DATABASE.md).
//!
//! Work distribution: the queue is de-interleaved round-robin into one
//! slice per worker (item i → worker i % N), which needs no channels and
//! balances both item count and, on average, per-item cost. Cancel is
//! cooperative: each worker re-checks the flag before every item and
//! breaks on the first cancelled tick (remaining items simply drop).

pub mod metrics;

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use image::{ImageBuffer, ImageReader, Rgb};

use crate::database::{AnalysisWork, Db};
use crate::error::{AppError, AppResult};
use crate::events::ProgressPayload;
use crate::scanner::DECODABLE_EXT;

/// Long-side cap for decode/measure: a 100 MP file must not explode memory.
/// 2048 px keeps the histogram pass + Laplacian well bounded while still
/// representing the frame's real content (IMAGE_ANALYSIS.md).
pub const WORKING_MAX_SIDE: u32 = 2048;
/// Concurrent decode+measure workers (at most this many images in RAM).
pub const ANALYSIS_WORKERS: usize = 3;
/// Refuse to decode absurdly large files into memory (≈500 MP guard).
const MAX_PIXELS: u64 = 500_000_000;
/// How many friendly per-file errors the summary carries (log has the rest).
const MAX_REPORTED_ERRORS: usize = 20;

/// Outcome of one analysis pass (carried in `analysis-complete`).
#[derive(Debug, Clone, serde::Serialize)]
pub struct AnalysisSummary {
    /// Photos newly measured by this run.
    pub analyzed: usize,
    /// Photos that failed to decode/measure (missing, corrupt, too large).
    pub failed: usize,
    /// True when the user stopped the run before the queue drained.
    pub cancelled: bool,
    pub elapsed_ms: u64,
    /// First few friendly messages; the log holds the full detail.
    pub errors: Vec<String>,
}

/// Run the full analysis pass over the queued photos.
///
/// `progress` is called from worker threads after each completed/failed
/// item, so it must be `Fn + Send + Sync` (the command layer wraps its
/// event emit in an `Arc`). See the module docs for cancel semantics.
pub fn run_analysis(
    db: Arc<Db>,
    progress: Arc<dyn Fn(ProgressPayload) + Send + Sync>,
    cancel: Arc<AtomicBool>,
) -> AppResult<AnalysisSummary> {
    let queue = db.analysis_queue(&DECODABLE_EXT)?;
    let total = queue.len();
    let start = Instant::now();

    if total == 0 {
        progress(ProgressPayload::new(0, 0, "analyzing"));
        return Ok(AnalysisSummary {
            analyzed: 0,
            failed: 0,
            cancelled: false,
            elapsed_ms: 0,
            errors: vec![],
        });
    }
    progress(ProgressPayload::new(total, 0, "analyzing"));

    let done = Arc::new(AtomicUsize::new(0));
    let failed = Arc::new(AtomicUsize::new(0));
    let errors = Arc::new(Mutex::new(Vec::<String>::new()));

    // Round-robin de-interleave: worker i gets items i, i+N, i+2N, ....
    let n_workers = ANALYSIS_WORKERS.min(total);
    let mut threads = Vec::new();
    for w in 0..n_workers {
        let slice: Vec<AnalysisWork> = queue
            .iter()
            .enumerate()
            .filter(|(i, _)| i % n_workers == w)
            .map(|(_, item)| item.clone())
            .collect();
        let db = db.clone();
        let progress = progress.clone();
        let cancel = cancel.clone();
        let done = done.clone();
        let failed = failed.clone();
        let errors = errors.clone();
        threads.push(std::thread::spawn(move || {
            for work in slice {
                if cancel.load(Ordering::Relaxed) {
                    break; // cooperative stop; remaining items drop
                }
                let filename = work.filename.clone();
                match analyze_one(&db, &work) {
                    Ok(()) => {
                        let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                        let f = failed.load(Ordering::Relaxed);
                        progress(
                            ProgressPayload::new(total, d + f, "analyzing").with_current(filename),
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
                            ProgressPayload::new(total, d + 1, "analyzing").with_current(filename),
                        );
                        tracing::warn!(path = %work.path, %friendly, "analysis item failed");
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
            "{panicked} analysis worker thread(s) panicked"
        )));
    }

    let analyzed = done.load(Ordering::Relaxed);
    let nfailed = failed.load(Ordering::Relaxed);
    let cancelled = cancel.load(Ordering::Relaxed) && analyzed + nfailed < total;
    let errors = errors.lock().expect("errors mutex poisoned").clone();
    tracing::info!(total, analyzed, failed = nfailed, cancelled, "analysis pass finished");
    Ok(AnalysisSummary {
        analyzed,
        failed: nfailed,
        cancelled,
        elapsed_ms: start.elapsed().as_millis() as u64,
        errors,
    })
}

/// Decode one file at working resolution, measure it, store the row.
/// Errors are friendly (shown to the user); details stay in the log.
fn analyze_one(db: &Db, work: &AnalysisWork) -> Result<(), String> {
    let path = Path::new(&work.path);
    if !path.exists() {
        return Err(format!("{} — file not found", work.filename));
    }
    let (w, h) = image::image_dimensions(path).map_err(|e| {
        tracing::error!(path = %work.path, error = %e, "dimensions read failed");
        format!("{} — file could not be read", work.filename)
    })?;
    if u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err(format!(
            "{} — image is too large to analyze safely",
            work.filename
        ));
    }
    let img = ImageReader::open(path)
        .map_err(|e| {
            tracing::error!(path = %work.path, error = %e, "image open failed");
            format!("{} — file could not be read", work.filename)
        })?
        .with_guessed_format()
        .map_err(|_| format!("{} — format not supported", work.filename))?
        .decode()
        .map_err(|e| {
            tracing::error!(path = %work.path, error = %e, "image decode failed");
            format!("{} — file is not a readable image", work.filename)
        })?;

    let rgb: ImageBuffer<Rgb<u8>, Vec<u8>> = if w.max(h) > WORKING_MAX_SIDE {
        let scale = f64::from(WORKING_MAX_SIDE) / f64::from(w.max(h));
        let tw = u32::max(1, (f64::from(w) * scale) as u32);
        let th = u32::max(1, (f64::from(h) * scale) as u32);
        img.resize_exact(tw, th, image::imageops::FilterType::Triangle)
            .to_rgb8()
    } else {
        img.to_rgb8()
    };

    let m = metrics::measure(&rgb);
    db.upsert_analysis(work.photo_id, &m, work.file_mtime.as_deref())
        .map_err(|e| {
            tracing::error!(path = %work.path, error = %e, "analysis upsert failed");
            format!("{} — could not store the result", work.filename)
        })
}
