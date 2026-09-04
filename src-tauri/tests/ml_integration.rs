//! Sprint 9 (local intelligence) integration tests: the face-detection pass
//! end-to-end against the real embedded model, the incremental
//! stamp/re-detect rules, and the face/analysis row co-existence invariants.
//!
//! The pass tests require the ONNX Runtime to be installed on the test
//! machine (it is on the dev box); where unavailable they skip, because the
//! degradation contract is that the app — and its tests of the queue and
//! storage invariants — still work without it.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use image::{ImageBuffer, Rgb, RgbImage};
use photogremlin_lib::database::{Db, PhotoUpsert};
use photogremlin_lib::events::ProgressPayload;
use photogremlin_lib::ml::{self, FaceSummary};

const FIXTURE: &str = "tests/fixtures/face_portrait.jpg";
const EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "tif", "tiff"];
const MTIME: &str = "2026-01-01T00:00:00Z";
const MTIME_NEW: &str = "2026-06-01T00:00:00Z";

struct Env {
    root: PathBuf,
    db: Arc<Db>,
}

impl Env {
    fn new() -> Env {
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir()
            .join(format!("pg_sprint9_it_{n}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let db_path = root.join("db.sqlite");
        let db = Arc::new(Db::open(&db_path).unwrap());
        db.migrate().unwrap();
        Env { root, db }
    }

    /// Index one decodable photo at a chosen recorded mtime; returns its id.
    fn add_photo(&self, name: &str, extension: &str, bytes: &[u8], file_mtime: &str) -> i64 {
        let path = self.root.join(name);
        std::fs::write(&path, bytes).unwrap();
        let (w, h) = image::image_dimensions(&path).unwrap();
        self.db
            .upsert_photo(&PhotoUpsert {
                path: path.to_string_lossy().into_owned(),
                filename: name.to_string(),
                extension: extension.to_string(),
                size_bytes: Some(bytes.len() as i64),
                width: Some(w as i64),
                height: Some(h as i64),
                orientation: None,
                session_id: None,
                file_mtime: Some(file_mtime.to_string()),
            })
            .unwrap()
    }
}

fn run_pass(db: &Arc<Db>) -> FaceSummary {
    ml::run_faces_pass(
        db.clone(),
        Arc::new(|_p: ProgressPayload| {}),
        Arc::new(AtomicBool::new(false)),
    )
    .expect("face pass returned Err (see friendly message in log)")
}

/// Run the pass honouring an external cancel flag (for the pre-cancel test).
fn run_pass_cancellable(db: &Arc<Db>, cancel: Arc<AtomicBool>) -> FaceSummary {
    ml::run_faces_pass(
        db.clone(),
        Arc::new(|_p: ProgressPayload| {}),
        cancel,
    )
    .expect("face pass returned Err (see friendly message in log)")
}

/// A procedural striped JPEG — rich in edges, zero faces by construction.
fn synthetic_jpeg(name: &str, w: u32, h: u32) -> Vec<u8> {
    let img: RgbImage = ImageBuffer::from_fn(w, h, |x, y| {
        let v = (((x + y) % 7) * 30 + 15) as u8;
        Rgb([v, v.saturating_sub(12), v.saturating_add(8)])
    });
    let mut out: Vec<u8> = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85)
        .encode(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    let _ = name;
    out
}

fn fixture_bytes() -> Vec<u8> {
    std::fs::read(FIXTURE).expect("face fixture must be committed in the repo")
}

// ---------------------------------------------------------------------------
// Runtime availability
// ---------------------------------------------------------------------------

#[test]
fn runtime_status_is_consistent_with_the_pass() {
    let env = Env::new();
    match ml::runtime_status() {
        Ok(()) => {
            // With the runtime present, an empty library is a clean no-op.
            let summary = run_pass(&env.db);
            assert_eq!(summary.processed, 0);
            assert!(summary.errors.is_empty());
        }
        Err(note) => {
            // Without it, the pass fails fast with the same friendly note —
            // and the app core is unaffected (queue + storage below).
            let result = ml::run_faces_pass(
                env.db.clone(),
                Arc::new(|_p: ProgressPayload| {}),
                Arc::new(AtomicBool::new(false)),
            );
            let msg = result.expect_err("pass must fail gracefully without the runtime");
            assert_eq!(msg.to_string(), note, "pass error and status note should agree");
        }
    }
}

// ---------------------------------------------------------------------------
// Queue + storage invariants (no runtime needed)
// ---------------------------------------------------------------------------

#[test]
fn face_only_row_is_picked_up_by_analysis_and_preserved() {
    let env = Env::new();
    let id = env.add_photo("portrait.jpg", "jpg", &fixture_bytes(), MTIME);

    // The face pass stamps a result before any analysis has run.
    env.db.upsert_faces(id, 7, Some(MTIME)).unwrap();
    let full = env.db.get_photo_full(id).unwrap();
    assert_eq!(full.face_count, Some(7));
    assert_eq!(full.sharpness, None, "measurements stay NULL until analyzed");

    // Face-only rows re-enter the analysis queue (source_mtime is NULL).
    let queued: Vec<i64> = env
        .db
        .analysis_queue(EXTENSIONS)
        .unwrap()
        .into_iter()
        .map(|w| w.photo_id)
        .collect();
    assert!(queued.contains(&id), "face-only row must be queued for analysis");

    // Then the analysis pass runs: it fills the measurements and must NOT
    // clobber the stored face result or its stamp.
    let metrics = photogremlin_lib::analysis::metrics::Metrics {
        sharpness: 41.0,
        brightness: 52.0,
        contrast: 63.0,
        saturation: 22.0,
        color_signature: 1,
        highlight_clipping: 1.0,
        shadow_clipping: 0.0,
        is_monochrome: false,
        is_dark: false,
        is_bright: false,
    };
    env.db
        .upsert_analysis(id, &metrics, Some(MTIME))
        .unwrap();
    let full = env.db.get_photo_full(id).unwrap();
    assert_eq!(full.face_count, Some(7), "analysis must keep the face result");
    assert_eq!(full.sharpness, Some(41.0));

    // Both queues are now clean for this photo.
    let queued: Vec<i64> = env
        .db
        .analysis_queue(EXTENSIONS)
        .unwrap()
        .into_iter()
        .map(|w| w.photo_id)
        .collect();
    assert!(!queued.contains(&id), "analyzed + stamped row must not be re-queued");
    let faces: Vec<i64> = env
        .db
        .faces_queue()
        .unwrap()
        .into_iter()
        .map(|w| w.photo_id)
        .collect();
    assert!(!faces.contains(&id), "fresh stamp must not be re-queued");

    // The status counter reflects the stored result.
    assert_eq!(env.db.status().unwrap().faces_done, 1);
}

#[test]
fn upsert_faces_is_idempotent_and_never_touches_source_mtime() {
    let env = Env::new();
    let id = env.add_photo("portrait.jpg", "jpg", &fixture_bytes(), MTIME);
    env.db.upsert_faces(id, 1, Some(MTIME)).unwrap();
    env.db.upsert_faces(id, 3, Some(MTIME_NEW)).unwrap();
    let full = env.db.get_photo_full(id).unwrap();
    assert_eq!(full.face_count, Some(3), "second stamp wins");
    // photos.file_mtime is owned by the scanner; a face stamp must not move it.
    assert_eq!(full.file_mtime.as_deref(), Some(MTIME));
}

// ---------------------------------------------------------------------------
// Full pass (require ONNX Runtime; skip gracefully when absent)
// ---------------------------------------------------------------------------

#[test]
fn pass_counts_the_fixture_face_and_zeroes_synthetic_photos() {
    if ml::runtime_status().is_err() {
        eprintln!("skipping: ONNX Runtime not installed on this machine");
        return;
    }
    let env = Env::new();
    let face_id = env.add_photo("portrait.jpg", "jpg", &fixture_bytes(), MTIME);
    let a_id = env.add_photo("pattern_a.jpg", "jpg", &synthetic_jpeg("pattern_a", 320, 240), MTIME);
    let b_id = env.add_photo("pattern_b.jpg", "jpg", &synthetic_jpeg("pattern_b", 240, 320), MTIME);

    let summary = run_pass(&env.db);
    assert_eq!(summary.processed, 3);
    assert_eq!(summary.with_faces, 1, "exactly the portrait has a face");
    assert_eq!(summary.failed, 0);
    assert!(!summary.cancelled);
    assert!(summary.errors.is_empty());

    let face = env.db.get_photo_full(face_id).unwrap();
    assert!(face.face_count.expect("face stored") >= 1, "portrait must have >= 1 face");
    let a = env.db.get_photo_full(a_id).unwrap();
    let b = env.db.get_photo_full(b_id).unwrap();
    assert_eq!(a.face_count, Some(0), "synthetic pattern must score 0 faces");
    assert_eq!(b.face_count, Some(0));
    assert_eq!(a.file_mtime.as_deref(), Some(MTIME));
}

#[test]
fn incremental_pass_is_noop_then_requeues_on_newer_mtime() {
    if ml::runtime_status().is_err() {
        eprintln!("skipping: ONNX Runtime not installed on this machine");
        return;
    }
    let env = Env::new();
    let face_id = env.add_photo("portrait.jpg", "jpg", &fixture_bytes(), MTIME);
    let b_id = env.add_photo("pattern_b.jpg", "jpg", &synthetic_jpeg("pattern_b", 240, 320), MTIME);

    let first = run_pass(&env.db);
    assert_eq!(first.processed, 2);

    // Nothing changed → the queue is empty (the whole value of the stamp).
    let second = run_pass(&env.db);
    assert_eq!(second.processed, 0, "unchanged photos must not be re-detected");

    // A newer recorded mtime re-queues exactly that photo.
    env.db
        .upsert_photo(&PhotoUpsert {
            path: env.root.join("pattern_b.jpg").to_string_lossy().into_owned(),
            filename: "pattern_b.jpg".into(),
            extension: "jpg".into(),
            size_bytes: Some(123),
            width: None,
            height: None,
            orientation: None,
            session_id: None,
            file_mtime: Some(MTIME_NEW.to_string()),
        })
        .unwrap();
    let queued: Vec<i64> = env
        .db
        .faces_queue()
        .unwrap()
        .into_iter()
        .map(|w| w.photo_id)
        .collect();
    assert_eq!(queued, vec![b_id], "only the touched photo is re-queued");

    let third = run_pass(&env.db);
    assert_eq!(third.processed, 1);
    let b = env.db.get_photo_full(b_id).unwrap();
    assert_eq!(b.face_count, Some(0));
    let face = env.db.get_photo_full(face_id).unwrap();
    assert!(face.face_count.unwrap() >= 1, "untouched portrait keeps its result");
}

#[test]
fn pre_set_cancel_stops_before_any_work() {
    if ml::runtime_status().is_err() {
        eprintln!("skipping: ONNX Runtime not installed on this machine");
        return;
    }
    let env = Env::new();
    env.add_photo("portrait.jpg", "jpg", &fixture_bytes(), MTIME);
    let cancel = Arc::new(AtomicBool::new(true));
    let summary = run_pass_cancellable(&env.db, cancel);
    assert!(summary.cancelled);
    assert_eq!(summary.processed, 0);
    assert_eq!(summary.failed, 0);
    let faces: Vec<i64> = env.db.faces_queue().unwrap().into_iter().map(|w| w.photo_id).collect();
    assert_eq!(faces.len(), 1, "cancelled work stays queued");
}

#[test]
fn missing_file_is_a_friendly_failure_not_a_stamp() {
    if ml::runtime_status().is_err() {
        eprintln!("skipping: ONNX Runtime not installed on this machine");
        return;
    }
    let env = Env::new();
    let id = env.add_photo("portrait.jpg", "jpg", &fixture_bytes(), MTIME);
    assert_eq!(env.root.join("portrait.jpg").exists(), true);
    std::fs::remove_file(env.root.join("portrait.jpg")).unwrap();

    let summary = run_pass(&env.db);
    assert_eq!(summary.processed, 0);
    assert_eq!(summary.failed, 1);
    assert!(
        summary.errors.first().map(|s| s.contains("file not found")) == Some(true),
        "friendly message expected, got {:?}",
        summary.errors
    );
    let full = env.db.get_photo_full(id).unwrap();
    assert_eq!(full.face_count, None, "a missing file must not be stamped 0");
}
