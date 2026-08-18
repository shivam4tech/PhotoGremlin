//! Local visual intelligence (Sprint 9): face detection.
//!
//! An optional, isolated layer: every core feature works with it disabled.
//! The model is a small (232 KB) local ONNX face detector (YuNet, Apache-2.0,
//! from the OpenCV Zoo) shipped with the app and run by ONNX Runtime loaded
//! from the system at runtime — zero network, zero telemetry.
//!
//! Pipeline (reference-validated against OpenCV's FaceDetectorYN, see
//! LOCAL_AI.md): resize to a fixed 640×640 (aspect distorted, bilinear) →
//! BGR channel order with per-channel mean (104, 177, 123) subtraction →
//! model → per-scale (stride 8/16/32) decode with offset-0 anchors,
//! stride-scaled deltas and `exp` box sizes, score `sqrt(cls·obj)` (the
//! sigmoids are inside the graph) → cross-scale NMS (IoU 0.3) → count.
//! Results map back to original pixels by the inverse resize factors.
//!
//! Incremental rule (mirrors the similarity pass, on `analysis.faces_at`):
//! a photo is (re-)detected iff it has no `face_count`, or the file's
//! `file_mtime` is newer than the mtime recorded when it was last detected.
//! Files above the size/pixel guards are stamped `face_count = 0` (with a
//! log line) so the queue does not re-attempt them forever; genuinely
//! unreadable files are a friendly per-file failure (retried next run).
//!
//! If the ONNX Runtime library is missing on a machine, detection reports a
//! friendly "unavailable" status and every other part of the app works.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use image::imageops::{resize, FilterType};
use image::{image_dimensions, ImageReader, RgbImage};
use ort::environment::Environment;
use ort::session::Session;
use ort::value::DynValue;

use crate::database::{Db, FaceWork};
use crate::error::{AppError, AppResult};
use crate::events::ProgressPayload;

/// The model's fixed input (the 2023mar export is not dynamic-sized).
const INPUT_SIZE: u32 = 640;
/// Per-channel mean, OpenCV BGR order (the reference pipeline does not
/// swap to RGB — the blob stays B,G,R with the canonical BGR mean).
const MEAN_BGR: [f32; 3] = [104.0, 177.0, 123.0];
/// Minimum `sqrt(cls·obj)` for a detection (OpenCV reference default).
pub const FACE_SCORE_THRESHOLD: f32 = 0.7;
/// Suppress a box overlapping a higher-scored one by more than this IoU.
pub const FACE_NMS_THRESHOLD: f32 = 0.3;
/// Boxes kept before NMS (plenty; the pass counts, never renders, boxes).
const FACE_TOP_K: usize = 100;
/// Detection head strides (feature map 640/s per side).
const STRIDES: [u32; 3] = [8, 16, 32];
/// Files this big are stamped 0 faces instead of streaming-parsed.
const MAX_FACE_FILE_BYTES: u64 = 256 * 1024 * 1024;
/// Decode guard (the blob is 640×640 anyway; this bounds decode memory).
const MAX_FACE_PIXELS: u64 = 250_000_000;
/// First few friendly per-file messages the summary carries (the log has all).
const MAX_REPORTED_ERRORS: usize = 20;

/// The local model, embedded in the binary (~232 KB; Apache-2.0 — provenance
/// and integrity hash pinned in docs/LOCAL_AI.md).
const MODEL: &[u8] =
    include_bytes!("../../models/face_detection_yunet_2023mar.onnx");

/// Candidate ONNX Runtime library names, highest priority first. The
/// canonical soname comes first; on Linux some installs ship only the
/// versioned file (no soname symlink), so standard lib dirs are scanned for
/// `libonnxruntime.so.1.*`.
fn runtime_lib_candidates() -> Vec<String> {
    let canonical: &str = if cfg!(target_os = "windows") {
        "onnxruntime.dll"
    } else if cfg!(target_os = "macos") {
        "libonnxruntime.dylib"
    } else {
        "libonnxruntime.so.1"
    };
    let mut out = vec![canonical.to_string()];
    if cfg!(all(unix, not(target_os = "macos"))) {
        let dirs = [
            "/usr/lib/x86_64-linux-gnu",
            "/usr/lib/aarch64-linux-gnu",
            "/usr/lib64",
            "/usr/lib",
            "/usr/local/lib",
        ];
        let prefix = "libonnxruntime.so.1.";
        let mut found: Vec<(u32, String)> = Vec::new();
        for dir in dirs {
            if let Ok(rd) = std::fs::read_dir(dir) {
                for e in rd.flatten() {
                    let name = e.file_name().to_string_lossy().into_owned();
                    if let Some(v) = name.strip_prefix(prefix) {
                        if let Ok(num) = v.parse::<u32>() {
                            found.push((num, name));
                        }
                    }
                }
            }
        }
        found.sort_by(|a, b| b.0.cmp(&a.0)); // newest minor first
        out.extend(found.into_iter().map(|(_, n)| n));
    }
    out
}

/// The library name that loads on this machine (probed once). `ort` PANICS
/// (not `Err`) when its dlopen fails, so the probe must run before any
/// `ort` call — this is what keeps the app alive when the runtime is absent.
static RUNTIME_LIB: OnceLock<Option<String>> = OnceLock::new();

fn resolve_runtime_lib() -> Option<String> {
    RUNTIME_LIB.get_or_init(|| {
        let mut found = None;
        for cand in runtime_lib_candidates() {
            match unsafe { libloading::Library::new(&cand) } {
                Ok(_lib) => {
                    found = Some(cand);
                    break;
                }
                Err(_) => continue,
            }
        }
        found
    })
    .clone()
}

/// The ONNX Runtime environment, loaded once from the system shared
/// library (no bundled runtime). Held for the process lifetime because
/// sessions parent on it.
static ENV: OnceLock<Result<Arc<Environment>, String>> = OnceLock::new();

/// Load (or reuse) the runtime; friendly error when it is missing.
fn ensure_runtime() -> AppResult<Arc<Environment>> {
    ENV.get_or_init(|| {
        let Some(lib) = resolve_runtime_lib() else {
            return Err(
                "the local ONNX Runtime library (libonnxruntime) is not available on this \
                 machine. Face detection is off; everything else in PhotoGremlin works \
                 normally. Install an ONNX Runtime distribution to enable it."
                    .to_string(),
            );
        };
        ort::init_from(&lib)
            .commit()
            .map_err(|e| format!("the local ONNX Runtime could not be started: {e}"))
    })
    .clone()
    .map_err(|note| AppError::validation(note))
}

/// `Ok(())` when face detection can run on this machine; the friendly
/// reason string otherwise (shown in Settings, never an app-wide error).
pub fn runtime_status() -> Result<(), String> {
    ensure_runtime().map(|_| ()).map_err(|e| e.to_string())
}

/// Size of the embedded model (reported in the Settings card).
pub fn model_bytes() -> usize {
    MODEL.len()
}

/// One detected face (x1, y1, w, h, score) in the model's 640² input space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceBox {
    pub x1: f32,
    pub y1: f32,
    pub w: f32,
    pub h: f32,
    pub score: f32,
}

/// IoU of two axis-aligned boxes (0.0 when disjoint).
pub fn iou(a: &FaceBox, b: &FaceBox) -> f32 {
    let x1 = a.x1.max(b.x1);
    let y1 = a.y1.max(b.y1);
    let x2 = (a.x1 + a.w).min(b.x1 + b.w);
    let y2 = (a.y1 + a.h).min(b.y1 + b.h);
    let inter = ((x2 - x1).max(0.0) * (y2 - y1).max(0.0)).max(0.0);
    let ua = a.w * a.h + b.w * b.h - inter;
    if ua <= 0.0 {
        0.0
    } else {
        inter / ua
    }
}

/// Non-max suppression: highest score first, suppress by IoU, keep `top_k`,
/// out sorted by score (ties by position) so results are deterministic.
pub fn nms(mut faces: Vec<FaceBox>, threshold: f32, top_k: usize) -> Vec<FaceBox> {
    faces.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.x1.partial_cmp(&b.x1).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.y1.partial_cmp(&b.y1).unwrap_or(std::cmp::Ordering::Equal))
    });
    let mut keep: Vec<FaceBox> = Vec::new();
    for f in faces {
        if keep.iter().all(|k| iou(&f, k) <= threshold) {
            keep.push(f);
            if keep.len() >= top_k {
                break;
            }
        }
    }
    keep
}

/// Decode the raw YuNet heads into NMS'd boxes in blob space.
///
/// `obj`, `cls`, `bbox` are indexed by stride position (0 = stride 8): each
/// score/head map is the flattened grid (640/s)², bbox is grid² × 4
/// (dx, dy, dw, dh). The official decode (libfacedetection): anchors at cell
/// corners (`c·stride`, `r·stride` — offset 0), center = delta·stride +
/// anchor, size = exp(delta)·stride. Score: `sqrt(cls·obj)` — the sigmoids
/// are inside the graph, so there is none in post-processing (verified:
/// double-sigmoiding collapses the score against the OpenCV reference).
pub fn decode_detections(
    obj: &[&[f32]],
    cls: &[&[f32]],
    bbox: &[&[f32]],
    score_threshold: f32,
    nms_threshold: f32,
    top_k: usize,
) -> Vec<FaceBox> {
    let mut cand = Vec::new();
    for (k, &stride) in STRIDES.iter().enumerate() {
        let grid = (INPUT_SIZE / stride) as usize;
        for r in 0..grid {
            for c in 0..grid {
                let i = r * grid + c;
                let score = (obj[k].get(i).copied().unwrap_or(0.0) * cls[k].get(i).copied().unwrap_or(0.0)).sqrt();
                if score < score_threshold {
                    continue;
                }
                let s = stride as f32;
                let base = i * 4;
                let b0 = bbox[k].get(base).copied().unwrap_or(0.0);
                let b1 = bbox[k].get(base + 1).copied().unwrap_or(0.0);
                let b2 = bbox[k].get(base + 2).copied().unwrap_or(0.0);
                let b3 = bbox[k].get(base + 3).copied().unwrap_or(0.0);
                let ax = c as f32 * s;
                let ay = r as f32 * s;
                let cx = b0 * s + ax;
                let cy = b1 * s + ay;
                let w = b2.exp() * s;
                let h = b3.exp() * s;
                cand.push(FaceBox {
                    x1: cx - w / 2.0,
                    y1: cy - h / 2.0,
                    w,
                    h,
                    score,
                });
            }
        }
    }
    nms(cand, nms_threshold, top_k)
}

/// The model's fixed 640² input from a photo's pixels: distorted (non
/// aspect-preserving) linear resize, BGR order, per-channel mean
/// subtracted, CHW float32. (`image`'s `Triangle` filter is linear
/// interpolation — the OpenCV INTER_LINEAR equivalent.)
pub fn build_blob(rgb: &RgbImage) -> Vec<f32> {
    let resized = resize(rgb, INPUT_SIZE, INPUT_SIZE, FilterType::Triangle);
    let n = (INPUT_SIZE * INPUT_SIZE) as usize;
    let mut out = vec![0.0f32; 3 * n];
    for (i, px) in resized.pixels().enumerate() {
        // `image` channels are [R, G, B]; the blob keeps the OpenCV BGR order.
        out[i] = px[2] as f32 - MEAN_BGR[0];
        out[n + i] = px[1] as f32 - MEAN_BGR[1];
        out[2 * n + i] = px[0] as f32 - MEAN_BGR[2];
    }
    out
}

/// One loaded model (a session over the embedded ONNX). `ort`'s `Session`
/// is `Send + Sync`, so a single detector serves the pass.
pub struct FaceDetector {
    session: Session,
}

impl FaceDetector {
    fn new(model: &[u8]) -> AppResult<Self> {
        ensure_runtime()?;
        let session = Session::builder()
            .and_then(|b| b.commit_from_memory(model))
            .map_err(|e| {
                AppError::operation(format!("the face model could not be loaded: {e}"))
            })?;
        Ok(Self { session })
    }

    /// Detect faces; boxes come back in original-image pixels.
    fn detect(&self, rgb: &RgbImage, orig_w: u32, orig_h: u32) -> AppResult<Vec<FaceBox>> {
        let blob = build_blob(rgb);
        let shape: Vec<i64> = vec![1, 3, INPUT_SIZE as i64, INPUT_SIZE as i64];
        let input: DynValue = (shape, blob.as_slice())
            .try_into()
            .map_err(|e: ort::Error| AppError::operation(format!("model input failed: {e}")))?;
        let inputs = ort::inputs![input]
            .map_err(|e| AppError::operation(format!("model input failed: {e}")))?;
        let outputs = self
            .session
            .run(inputs)
            .map_err(|e| AppError::operation(format!("face inference failed: {e}")))?;

        let head = |name: &str| -> AppResult<Vec<f32>> {
            let (_, data) = outputs[name]
                .try_extract_raw_tensor::<f32>()
                .map_err(|e| AppError::operation(format!("reading {name} failed: {e}")))?;
            Ok(data.to_vec())
        };
        let mut obj: Vec<Vec<f32>> = vec![Vec::new(); 3];
        let mut cls: Vec<Vec<f32>> = vec![Vec::new(); 3];
        let mut bbox: Vec<Vec<f32>> = vec![Vec::new(); 3];
        for (k, stride) in STRIDES.iter().enumerate() {
            obj[k] = head(&format!("obj_{stride}"))?;
            cls[k] = head(&format!("cls_{stride}"))?;
            bbox[k] = head(&format!("bbox_{stride}"))?;
        }
        let boxes = decode_detections(
            &[&obj[0], &obj[1], &obj[2]],
            &[&cls[0], &cls[1], &cls[2]],
            &[&bbox[0], &bbox[1], &bbox[2]],
            FACE_SCORE_THRESHOLD,
            FACE_NMS_THRESHOLD,
            FACE_TOP_K,
        );
        // Blob → original pixels (the blob is the photo stretched to 640²).
        let sx = orig_w as f32 / INPUT_SIZE as f32;
        let sy = orig_h as f32 / INPUT_SIZE as f32;
        Ok(boxes
            .into_iter()
            .map(|b| FaceBox {
                x1: b.x1 * sx,
                y1: b.y1 * sy,
                w: b.w * sx,
                h: b.h * sy,
                score: b.score,
            })
            .collect())
    }
}

/// Outcome of one detection pass (carried in `faces-complete`).
#[derive(Debug, Clone, serde::Serialize)]
pub struct FaceSummary {
    /// Photos the pass inspected this run (including zero-face results).
    pub processed: usize,
    /// Photos of those with ≥ 1 detected face.
    pub with_faces: usize,
    /// Photos that failed (missing, unreadable, inference error).
    pub failed: usize,
    /// True when the user stopped the run before the queue drained.
    pub cancelled: bool,
    pub elapsed_ms: u64,
    /// First few friendly messages; the log holds the full detail.
    pub errors: Vec<String>,
}

/// Run the face-detection pass over the queued photos. Sequential by design
/// (one decode + one 640² inference per file through a single shared
/// session; a few hundred photos per minute on CPU — v0.1 keeps it simple
/// and deterministic, and the queue stays small because it is incremental).
pub fn run_faces_pass(
    db: Arc<Db>,
    progress: Arc<dyn Fn(ProgressPayload) + Send + Sync>,
    cancel: Arc<AtomicBool>,
) -> AppResult<FaceSummary> {
    let started = Instant::now();

    // Fail fast with the friendly runtime note (nothing half-processed).
    ensure_runtime()?;
    let detector = FaceDetector::new(MODEL)?;

    let queue = db.faces_queue()?;
    let total = queue.len();
    if total == 0 {
        progress(ProgressPayload::new(0, 0, "detecting faces"));
        return Ok(FaceSummary {
            processed: 0,
            with_faces: 0,
            failed: 0,
            cancelled: false,
            elapsed_ms: 0,
            errors: vec![],
        });
    }
    progress(ProgressPayload::new(total, 0, "detecting faces"));

    let mut processed = 0usize;
    let mut with_faces = 0usize;
    let mut failed = 0usize;
    let mut cancelled = false;
    let errors: Mutex<Vec<String>> = Mutex::new(Vec::new());
    for w in &queue {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            tracing::info!("face pass cancelled between files");
            break;
        }
        let filename = Path::new(&w.path)
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        match process_one(&db, &detector, w) {
            Ok(count) => {
                processed += 1;
                if count > 0 {
                    with_faces += 1;
                }
            }
            Err(friendly) => {
                failed += 1;
                {
                    let mut g = errors.lock().expect("errors mutex poisoned");
                    if g.len() < MAX_REPORTED_ERRORS {
                        g.push(friendly.clone());
                    }
                }
                tracing::warn!(path = %w.path, %friendly, "face item failed");
            }
        }
        let done = processed + failed;
        progress(
            ProgressPayload::new(total, done, "detecting faces").with_current(filename),
        );
    }

    let errors = errors
        .into_inner()
        .expect("errors mutex poisoned")
        .into_iter()
        .collect::<Vec<_>>();
    tracing::info!(
        total,
        processed,
        with_faces,
        failed,
        cancelled,
        "face pass finished"
    );
    Ok(FaceSummary {
        processed,
        with_faces,
        failed,
        cancelled,
        elapsed_ms: started.elapsed().as_millis() as u64,
        errors,
    })
}

/// Inspect one file: decode → detect → store the count. Friendly `Err`
/// (counted + logged by the caller) on any failure; oversize/over-pixel
/// files are stamped `face_count = 0` and count as processed (the metadata
/// pass's "stamp it so the queue does not re-attempt forever" rule).
fn process_one(db: &Db, det: &FaceDetector, w: &FaceWork) -> Result<u32, String> {
    let path = Path::new(&w.path);
    let name = || {
        path.file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    if !path.exists() {
        return Err(format!("{} — file not found", name()));
    }
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if size > MAX_FACE_FILE_BYTES {
        return guard_stamp(db, w, "file too large for face detection");
    }
    // Header-only pixel guard before the full decode.
    let (dw, dh) = image_dimensions(path)
        .map_err(|_| format!("{} — image could not be read", name()))?;
    if (dw as u64) * (dh as u64) > MAX_FACE_PIXELS {
        return guard_stamp(db, w, "image too large for face detection");
    }
    let rgb = read_rgb(path).map_err(|_| format!("{} — image could not be decoded", name()))?;
    let count = det
        .detect(&rgb, dw, dh)
        .map(|boxes| boxes.len() as u32)
        .map_err(|_| format!("{} — face detection failed", name()))?;
    db.upsert_faces(w.photo_id, count as i64, w.file_mtime.as_deref())
        .map_err(|e| {
            tracing::error!(path = %w.path, error = %e, "face result store failed");
            format!("{} — could not store the result", name())
        })?;
    Ok(count)
}

fn read_rgb(path: &Path) -> image::ImageResult<RgbImage> {
    let reader = ImageReader::open(path).map_err(|_| {
        image::ImageError::IoError(std::io::Error::new(
            std::io::ErrorKind::Other,
            "could not open file",
        ))
    })?;
    Ok(reader.with_guessed_format()?.decode()?.to_rgb8())
}

/// Oversize/over-pixel: stamp `face_count = 0` and count as processed.
fn guard_stamp(db: &Db, w: &FaceWork, note: &str) -> Result<u32, String> {
    tracing::info!(photo = w.photo_id, %note, "face guard: stamped 0 faces");
    db.upsert_faces(w.photo_id, 0, w.file_mtime.as_deref())
        .map_err(|e| {
            tracing::error!(photo = w.photo_id, error = %e, "face guard stamp failed");
            format!("could not store the result")
        })?;
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};

    fn box_(x: f32, y: f32, w: f32, h: f32, s: f32) -> FaceBox {
        FaceBox { x1: x, y1: y, w, h, score: s }
    }

    #[test]
    fn build_blob_is_chw_bgr_mean_subtracted() {
        // 1×1 image: B=10, G=20, R=30 → blob channels start (10-104, 20-177, 30-123).
        let img: RgbImage = ImageBuffer::from_pixel(1, 1, Rgb([30, 20, 10]));
        let blob = build_blob(&img);
        assert_eq!(blob.len(), 3 * (INPUT_SIZE as usize) * (INPUT_SIZE as usize));
        let n = (INPUT_SIZE as usize) * (INPUT_SIZE as usize);
        // The single pixel is replicated to every cell by the resize.
        assert!((blob[0] - (10.0f32 - MEAN_BGR[0])).abs() < 1e-3);
        assert!((blob[n] - (20.0 - MEAN_BGR[1])).abs() < 1e-3);
        assert!((blob[2 * n] - (30.0 - MEAN_BGR[2])).abs() < 1e-3);
    }

    #[test]
    fn decode_uses_offset_zero_anchors_stride_deltas_exp_sizes() {
        // One confident cell at (r=2, c=3) on stride 16, everything else 0.
        // Each stride head must carry its own grid size (640/s)².
        let m8 = (INPUT_SIZE / 8) as usize;
        let m16 = (INPUT_SIZE / 16) as usize;
        let m32 = (INPUT_SIZE / 32) as usize;
        let zero8 = vec![0.0f32; m8 * m8];
        let _zero16 = vec![0.0f32; m16 * m16];
        let zero32 = vec![0.0f32; m32 * m32];
        let mut obj16 = vec![0.0f32; m16 * m16];
        let mut cls16 = vec![0.0f32; m16 * m16];
        let mut bbox16 = vec![0.0f32; m16 * m16 * 4];
        let i = 2 * m16 + 3;
        obj16[i] = 0.9;
        cls16[i] = 1.0; // score = sqrt(0.9) ≈ 0.949 ≥ 0.7
        let s = 16.0f32;
        bbox16[i * 4] = 0.2; // dx
        bbox16[i * 4 + 1] = -0.25; // dy
        bbox16[i * 4 + 2] = (2.0f32).ln(); // w → 2·stride
        bbox16[i * 4 + 3] = (1.5f32).ln(); // h → 1.5·stride
        let faces = decode_detections(
            &[&zero8, &obj16, &zero32],
            &[&zero8, &cls16, &zero32],
            &[&vec![0.0f32; m8 * m8 * 4], &bbox16, &vec![0.0f32; m32 * m32 * 4]],
            FACE_SCORE_THRESHOLD,
            FACE_NMS_THRESHOLD,
            FACE_TOP_K,
        );
        assert_eq!(faces.len(), 1);
        let f = faces[0];
        let cx = 0.2 * s + 3.0 * s;
        let cy = -0.25 * s + 2.0 * s;
        assert!((f.w - 2.0 * s).abs() < 1e-3);
        assert!((f.h - 1.5 * s).abs() < 1e-3);
        assert!((f.x1 - (cx - f.w / 2.0)).abs() < 1e-3);
        assert!((f.y1 - (cy - f.h / 2.0)).abs() < 1e-3);
        assert!((f.score - (0.9f32).sqrt()).abs() < 1e-3);
    }

    #[test]
    fn decode_filters_below_threshold() {
        let m8 = (INPUT_SIZE / 8) as usize;
        let m16 = (INPUT_SIZE / 16) as usize;
        let m32 = (INPUT_SIZE / 32) as usize;
        // sqrt(0.5·0.5) = 0.5 < 0.7 → no detection.
        let faces = decode_detections(
            &[&vec![0.5f32; m8 * m8], &[], &[]],
            &[&vec![0.5f32; m8 * m8], &[], &[]],
            &[&vec![0.0f32; m8 * m8 * 4], &vec![0.0f32; m16 * m16 * 4], &vec![0.0f32; m32 * m32 * 4]],
            FACE_SCORE_THRESHOLD,
            FACE_NMS_THRESHOLD,
            FACE_TOP_K,
        );
        assert!(faces.is_empty());
    }

    #[test]
    fn nms_suppresses_overlapping_lower_score_boxes() {
        let big = box_(0.0, 0.0, 100.0, 100.0, 0.9);
        let nested = box_(10.0, 10.0, 80.0, 80.0, 0.8); // IoU 0.64 > 0.3
        let apart = box_(300.0, 300.0, 50.0, 50.0, 0.75);
        let out = nms(vec![nested, apart.clone(), big.clone()], FACE_NMS_THRESHOLD, FACE_TOP_K);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], big);
        assert_eq!(out[1], apart);
    }

    #[test]
    fn iou_is_zero_when_disjoint_and_one_when_equal() {
        let a = box_(0.0, 0.0, 10.0, 10.0, 1.0);
        let b = box_(50.0, 50.0, 10.0, 10.0, 1.0);
        let c = box_(0.0, 0.0, 10.0, 10.0, 0.5);
        assert_eq!(iou(&a, &b), 0.0);
        assert!((iou(&a, &c) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn runtime_status_is_consistent_and_cached() {
        // Whatever this machine has, the probe answers identically twice
        // (the result is cached) and, when unavailable, says so in friendly
        // text naming the library (never a stack trace).
        let a = runtime_status().err();
        let b = runtime_status().err();
        assert_eq!(a, b);
        if let Some(note) = &a {
            assert!(note.contains("ONNX Runtime"), "note should name the runtime: {note}");
        }
    }
}
