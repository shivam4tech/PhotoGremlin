//! Scene classification (Sprint 18): the second optional local-AI layer.
//!
//! Mirrors the face-detection pattern exactly: a small ONNX model embedded
//! in the binary, run through ONNX Runtime (loaded from the system), fully
//! optional — every core feature works when the model or runtime is absent,
//! and `scene_*` columns stay NULL ("unavailable", never fake zeros).
//!
//! Model contract (produced by tools/train/export_onnx.py; a deterministic
//! random-weight stub ships until then):
//!   input  "image"  [batch, 3, 224, 224] float32, RGB, ImageNet-normalized
//!   output "fine"   [batch, N_FINE]      logits over scene labels
//!   output "coarse" [batch, N_COARSE]    logits over raw coarse groups
//! The stored `scene_coarse` is the MERGED product group (10 chips) mapped
//! from the raw head via [`MERGED_GROUPS`].
//!
//! Preprocess matches training-time eval: resize shortest side to 256,
//! center-crop 224×224, ImageNet mean/std normalization.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use image::imageops::FilterType;
use image::{ImageReader, RgbImage};
use ort::environment::Environment;
use ort::session::Session;
use ort::value::DynValue;
use serde::Serialize;

use crate::database::{Db, SceneWork};
use crate::error::{AppError, AppResult};
use crate::events::ProgressPayload;

/// The shipped scene model (placeholder stub until export_onnx.py lands the
/// trained artifact at the same path).
pub const SCENE_MODEL: &[u8] = include_bytes!("../../models/scene_mobilenetv3_large.onnx");
const SCENE_LABELS: &str = include_str!("../../models/scene_labels.json");

pub const SCENE_MODEL_NAME: &str = "MobileNetV3-Large two-head (trained on CC-BY Open Images)";

const INPUT_SIZE: u32 = 224;
const RESIZE_SHORT_SIDE: u32 = 256;
/// Guard mirrors the face pass: stamp-and-skip pathological files so the
/// queue does not re-attempt them forever.
const MAX_SCENE_FILE_BYTES: u64 = 256 * 1024 * 1024;

const IMAGENET_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const IMAGENET_STD: [f32; 3] = [0.229, 0.224, 0.225];

/// Raw coarse head groups (21) -> product filter chips (10). Identical to
/// the mapping used in evaluation; keep both sides in sync.
const MERGED_GROUPS: &[(&str, &str)] = &[
    ("nature", "nature"),
    ("nature_water", "nature"),
    ("urban", "urban"),
    ("indoor_home", "home_stay"),
    ("residential", "home_stay"),
    ("hotel", "home_stay"),
    ("indoor", "public_indoor"),
    ("indoor_cultural", "public_indoor"),
    ("indoor_retail", "public_indoor"),
    ("workplace", "public_indoor"),
    ("education", "public_indoor"),
    ("healthcare", "public_indoor"),
    ("religious", "faith_history"),
    ("historic", "faith_history"),
    ("sports", "sports_leisure"),
    ("sports_stadium", "sports_leisure"),
    ("food_dining", "food_night"),
    ("public_transport", "transport"),
    ("transport_vehicle", "transport"),
    ("industrial", "industry"),
    ("other", "other"),
];

fn merged_group(raw: &str) -> &'static str {
    MERGED_GROUPS
        .iter()
        .find(|(k, _)| *k == raw)
        .map(|(_, m)| *m)
        .unwrap_or("other")
}

#[derive(Debug, Clone)]
struct SceneLabels {
    fine: Vec<String>,
    coarse: Vec<String>,
}

impl SceneLabels {
    fn load() -> AppResult<Self> {
        let parsed: serde_json::Value = serde_json::from_str(SCENE_LABELS)
            .map_err(|e| AppError::operation(format!("scene labels unreadable: {e}")))?;
        let fine = parsed["fine_classes"]
            .as_array()
            .ok_or_else(|| AppError::operation("scene labels missing fine_classes"))?
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        let coarse = parsed["coarse_classes"]
            .as_array()
            .ok_or_else(|| AppError::operation("scene labels missing coarse_classes"))?
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        Ok(Self { fine, coarse })
    }
}

/// One pass summary (mirrors FaceSummary).
#[derive(Debug, Clone, Serialize)]
pub struct SceneSummary {
    pub processed: u32,
    pub failed: u32,
    pub cancelled: bool,
    pub elapsed_ms: u64,
    /// First few friendly messages; the log holds the full detail.
    pub errors: Vec<String>,
}

pub struct SceneClassifier {
    session: Session,
    labels: SceneLabels,
}

impl SceneClassifier {
    fn new(model: &[u8]) -> AppResult<Self> {
        crate::ml::ensure_runtime()?;
        let session = Session::builder()
            .and_then(|b| b.commit_from_memory(model))
            .map_err(|e| AppError::operation(format!("the scene model could not be loaded: {e}")))?;
        Ok(Self {
            session,
            labels: SceneLabels::load()?,
        })
    }

    /// Classify one decoded RGB image. Returns (merged_coarse, fine, conf).
    fn classify(&self, rgb: &RgbImage) -> AppResult<(String, String, f32)> {
        let blob = preprocess(rgb);
        let shape: Vec<i64> = vec![1, 3, INPUT_SIZE as i64, INPUT_SIZE as i64];
        let input: DynValue = (shape, blob.as_slice())
            .try_into()
            .map_err(|e: ort::Error| AppError::operation(format!("model input failed: {e}")))?;
        let inputs = ort::inputs![input]
            .map_err(|e| AppError::operation(format!("model input failed: {e}")))?;
        let outputs = self
            .session
            .run(inputs)
            .map_err(|e| AppError::operation(format!("scene inference failed: {e}")))?;

        let head = |name: &str| -> AppResult<Vec<f32>> {
            let (_, data) = outputs[name]
                .try_extract_raw_tensor::<f32>()
                .map_err(|e| AppError::operation(format!("reading {name} failed: {e}")))?;
            Ok(data.to_vec())
        };
        let fine_logits = head("fine")?;
        let coarse_logits = head("coarse")?;

        // softmax over the fine head for the confidence of its top-1 label
        let max = fine_logits.iter().cloned().fold(f32::MIN, f32::max);
        let exps: Vec<f32> = fine_logits.iter().map(|v| (v - max).exp()).collect();
        let sum: f32 = exps.iter().sum();
        let mut best_i = 0usize;
        let mut best_p = 0f32;
        for (i, e) in exps.iter().enumerate() {
            let p = e / sum;
            if p > best_p {
                best_p = p;
                best_i = i;
            }
        }
        let fine_label = self
            .labels
            .fine
            .get(best_i)
            .cloned()
            .ok_or_else(|| AppError::operation("fine label index out of range"))?;

        let c_max = coarse_logits.iter().cloned().fold(f32::MIN, f32::max);
        let c_best = coarse_logits
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, _)| i)
            .unwrap_or(0);
        let raw_group = self
            .labels
            .coarse
            .get(c_best)
            .map(String::as_str)
            .unwrap_or("other");
        let _ = c_max;

        Ok((merged_group(raw_group).to_string(), fine_label, best_p))
    }
}

/// Resize shortest side to 256, center-crop 224², CHW float32 normalized.
fn preprocess(rgb: &RgbImage) -> Vec<f32> {
    let (w, h) = (rgb.width(), rgb.height());
    let scale = RESIZE_SHORT_SIDE as f32 / w.min(h) as f32;
    let rw = ((w as f32 * scale).round() as u32).max(INPUT_SIZE);
    let rh = ((h as f32 * scale).round() as u32).max(INPUT_SIZE);
    let resized = image::imageops::resize(rgb, rw, rh, FilterType::Triangle);
    let left = (rw - INPUT_SIZE) / 2;
    let top = (rh - INPUT_SIZE) / 2;
    let crop =
        image::imageops::crop_imm(&resized, left, top, INPUT_SIZE, INPUT_SIZE).to_image();

    let mut blob = vec![0f32; 3 * (INPUT_SIZE * INPUT_SIZE) as usize];
    for (x, y, px) in crop.enumerate_pixels() {
        let (r, g, b) = (
            px[0] as f32 / 255.0,
            px[1] as f32 / 255.0,
            px[2] as f32 / 255.0,
        );
        let (x, y) = (x as usize, y as usize);
        blob[y * INPUT_SIZE as usize + x] = (r - IMAGENET_MEAN[0]) / IMAGENET_STD[0];
        blob[(INPUT_SIZE as usize + y) * INPUT_SIZE as usize + x] =
            (g - IMAGENET_MEAN[1]) / IMAGENET_STD[1];
        blob[(2 * INPUT_SIZE as usize + y) * INPUT_SIZE as usize + x] =
            (b - IMAGENET_MEAN[2]) / IMAGENET_STD[2];
    }
    blob
}

/// Run the scene-classification pass over the queued photos. Sequential by
/// design (one decode + one 224² inference per file through a single shared
/// session); incremental like every other pass.
pub fn run_scenes_pass(
    db: Arc<Db>,
    progress: Arc<dyn Fn(ProgressPayload) + Send + Sync>,
    cancel: Arc<AtomicBool>,
) -> AppResult<SceneSummary> {
    let started = Instant::now();
    crate::ml::ensure_runtime()?;
    let classifier = SceneClassifier::new(SCENE_MODEL)?;

    let queue = db.scenes_queue()?;
    let total = queue.len();
    if total == 0 {
        progress(ProgressPayload::new(0, 0, "classifying scenes"));
        return Ok(SceneSummary {
            processed: 0,
            failed: 0,
            cancelled: false,
            elapsed_ms: 0,
            errors: vec![],
        });
    }
    progress(ProgressPayload::new(total, 0, "classifying scenes"));

    let mut processed = 0u32;
    let mut failed = 0u32;
    let mut cancelled = false;
    let errors: Mutex<Vec<String>> = Mutex::new(Vec::new());
    for w in &queue {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            tracing::info!("scene pass cancelled between files");
            break;
        }
        let filename = Path::new(&w.path)
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        match process_one(&db, &classifier, w) {
            Ok(()) => processed += 1,
            Err(friendly) => {
                failed += 1;
                {
                    let mut g = errors.lock().expect("errors mutex poisoned");
                    if g.len() < 5 {
                        g.push(friendly.clone());
                    }
                }
                tracing::warn!(path = %w.path, %friendly, "scene item failed");
            }
        }
        let done = processed + failed;
        progress(
            ProgressPayload::new(total as usize, done as usize, "classifying scenes")
                .with_current(filename),
        );
    }

    let errors = errors
        .into_inner()
        .expect("errors mutex poisoned")
        .into_iter()
        .collect::<Vec<_>>();
    tracing::info!(total, processed, failed, cancelled, "scene pass finished");
    Ok(SceneSummary {
        processed,
        failed,
        cancelled,
        elapsed_ms: started.elapsed().as_millis() as u64,
        errors,
    })
}

/// Inspect one file: decode → classify → store. Friendly `Err` on failure;
/// oversize files are stamped like the face pass does.
fn process_one(db: &Db, clf: &SceneClassifier, w: &SceneWork) -> Result<(), String> {
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
    if size > MAX_SCENE_FILE_BYTES {
        tracing::info!(path = %w.path, size, "file above scene guard; skipping");
        return guard_stamp(db, w);
    }
    let img = ImageReader::open(path)
        .map_err(|e| format!("{} — could not open file ({e})", name()))?
        .decode()
        .map(|dyn_img| dyn_img.to_rgb8())
        .map_err(|e| format!("{} — could not decode image ({e})", name()))?;
    let (coarse, fine, conf) = clf.classify(&img).map_err(|e| {
        tracing::error!(path = %w.path, error = %e, "scene inference failed");
        format!("{} — could not classify scene", name())
    })?;
    db.upsert_scene(w.photo_id, &coarse, &fine, conf as f64, w.file_mtime.as_deref())
        .map_err(|e| {
            tracing::error!(photo = w.photo_id, error = %e, "scene upsert failed");
            "could not store the result".to_string()
        })
}

fn guard_stamp(db: &Db, w: &SceneWork) -> Result<(), String> {
    tracing::info!(photo = w.photo_id, "scene guard: stamped unavailable");
    db.upsert_scene(w.photo_id, "", "", 0.0, w.file_mtime.as_deref())
        .map_err(|e| {
            tracing::error!(photo = w.photo_id, error = %e, "scene guard stamp failed");
            "could not store the result".to_string()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn license_style_merged_mapping_covers_all_raw_groups() {
        for (raw, merged) in MERGED_GROUPS {
            assert_eq!(merged_group(raw), *merged);
        }
        assert_eq!(merged_group("unknown-group"), "other");
    }

    #[test]
    fn preprocess_output_is_normalized_chw() {
        let img = RgbImage::from_pixel(400, 300, image::Rgb([128, 128, 128]));
        let blob = preprocess(&img);
        assert_eq!(blob.len(), 3 * 224 * 224);
        // center pixel of a uniform gray image normalizes to ~0.074
        let mid = blob[112 * 224 + 112];
        assert!((0.05..0.10).contains(&mid), "mid={mid}");
    }
}
