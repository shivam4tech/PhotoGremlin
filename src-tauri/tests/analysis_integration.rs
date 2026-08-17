//! Integration test: synthetic folder → scan → analyze → verify.
//!
//! Covers the Sprint 4 acceptance path without touching real photos:
//! every decodable gets a current-version row, RAW is measured-eligible but
//! not decoded, re-runs are incremental by (algorithm_version, mtime),
//! failures are friendly, and cancel stops the pass cooperatively.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use image::ImageBuffer;
use photogremlin_lib::analysis::{self, AnalysisSummary};
use photogremlin_lib::database::Db;
use photogremlin_lib::events::ProgressPayload;
use photogremlin_lib::scanner;

fn solid_png(path: &Path, w: u32, h: u32, rgb: [u8; 3]) {
    ImageBuffer::from_pixel(w, h, image::Rgb(rgb)).save(path).unwrap();
}

fn ramp_png(path: &Path, w: u32, h: u32) {
    // Linear horizontal ramp: Laplacian ≈ 0 → very low sharpness.
    let img: ImageBuffer<image::Rgb<u8>, Vec<u8>> =
        ImageBuffer::from_fn(w, h, |x, _| {
            let v = (x % 256) as u8;
            image::Rgb([v, v, v])
        });
    img.save(path).unwrap();
}

fn checker_png(path: &Path, w: u32, h: u32, block: u32) {
    let img: ImageBuffer<image::Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(w, h, |x, y| {
        if (x / block + y / block) % 2 == 0 {
            image::Rgb([255, 255, 255])
        } else {
            image::Rgb([0, 0, 0])
        }
    });
    img.save(path).unwrap();
}

fn setup(label: &str) -> (PathBuf, PathBuf, PathBuf) {
    let base =
        std::env::temp_dir().join(format!("pg_analysis_it_{label}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let shoot = base.join("Shoot_Analysis");
    std::fs::create_dir_all(&shoot).unwrap();
    (base.clone(), base.join("db.sqlite"), shoot)
}

fn noop_progress() -> Arc<dyn Fn(ProgressPayload) + Send + Sync> {
    Arc::new(|_p: ProgressPayload| {})
}

fn counting_progress(
) -> (
    Arc<dyn Fn(ProgressPayload) + Send + Sync>,
    Arc<Mutex<Vec<(usize, usize)>>>,
) {
    let log = Arc::new(Mutex::new(Vec::<(usize, usize)>::new()));
    let cap = log.clone();
    (
        Arc::new(move |p: ProgressPayload| {
            cap.lock().unwrap().push((p.done, p.total));
        }),
        log,
    )
}

fn run_analysis(
    db: &Arc<Db>,
    progress: Arc<dyn Fn(ProgressPayload) + Send + Sync>,
    cancel: Arc<AtomicBool>,
) -> AnalysisSummary {
    analysis::run_analysis(db.clone(), progress, cancel).unwrap()
}

/// (sharpness, brightness, version, source_mtime, analyzed_at) for one file.
fn analysis_row(db: &Db, filename: &str) -> Option<(f64, f64, i64, Option<String>, Option<String>)> {
    let conn = db.lock().unwrap();
    conn.query_row(
        "SELECT a.sharpness, a.brightness, a.algorithm_version, a.source_mtime, a.analyzed_at
         FROM analysis a JOIN photos p ON p.id = a.photo_id
         WHERE p.filename = ?1",
        [filename],
        |r| {
            Ok((
                r.get::<_, Option<f64>>(0)?.unwrap_or(f64::NAN),
                r.get::<_, Option<f64>>(1)?.unwrap_or(f64::NAN),
                r.get::<_, i64>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        },
    )
    .ok()
}

fn build_shoot(shoot: &Path) {
    solid_png(&shoot.join("BRIGHT.png"), 320, 240, [245, 245, 245]);
    solid_png(&shoot.join("DARK.png"), 320, 240, [10, 10, 10]);
    checker_png(&shoot.join("SHARP.png"), 256, 256, 8);
    ramp_png(&shoot.join("SMOOTH.png"), 512, 512);
    std::fs::write(shoot.join("RAW_0001.CR3"), b"fake-raw-bytes").unwrap();
    std::fs::write(shoot.join("notes.txt"), b"ignore me").unwrap();
}

#[test]
fn analysis_measures_decodables_and_is_incremental() {
    let (base, db_path, shoot) = setup("main");
    build_shoot(&shoot);

    let db = Arc::new(Db::open(&db_path).unwrap());
    db.migrate().unwrap();

    let cancel = Arc::new(AtomicBool::new(false));
    scanner::run_scan(
        &shoot,
        &db,
        &mut |p: ProgressPayload| {
            let _ = p;
        },
        &cancel,
    )
    .unwrap();

    // First pass: all 4 decodables measured; RAW not decodable → not in queue.
    let (progress, log) = counting_progress();
    let s1 = run_analysis(&db, progress, cancel.clone());
    assert_eq!(s1.analyzed, 4, "summary {:?}, errors: {:?}", s1, s1.errors);
    assert_eq!(s1.failed, 0);
    assert!(!s1.cancelled);
    let entries = log.lock().unwrap().clone();
    assert!(!entries.is_empty());
    assert_eq!(*entries.last().unwrap(), (4, 4)); // final progress drained

    // Rows exist for exactly the 4 decodables, version 1, mtime recorded.
    let raw = analysis_row(&db, "RAW_0001.CR3");
    assert!(raw.is_none(), "RAW must not get an analysis row: {:?}", raw);
    for name in ["BRIGHT.png", "DARK.png", "SHARP.png", "SMOOTH.png"] {
        let (sharp, bright, ver, mtime, at) = analysis_row(&db, name).unwrap();
        assert_eq!(ver, 1, "{name} version");
        assert!(sharp.is_finite() && (0.0..=100.0).contains(&sharp), "{name} sharpness");
        assert!(bright.is_finite() && (0.0..=100.0).contains(&bright), "{name} brightness");
        assert!(mtime.is_some(), "{name} source_mtime recorded");
        assert!(at.is_some(), "{name} analyzed_at recorded");
    }

    // Values order sensibly (measured characteristics, not verdicts).
    let (_, bright, ..) = analysis_row(&db, "BRIGHT.png").unwrap();
    let (_, dark, ..) = analysis_row(&db, "DARK.png").unwrap();
    assert!(bright > 70.0, "bright: {bright}");
    assert!(dark < 10.0, "dark: {dark}");
    let (sharp_s, ..) = analysis_row(&db, "SHARP.png").unwrap();
    let (sharp_m, ..) = analysis_row(&db, "SMOOTH.png").unwrap();
    assert!(sharp_s > sharp_m + 10.0, "sharp {sharp_s} vs smooth {sharp_m}");

    // Second pass: nothing to do (same algorithm, unchanged mtimes).
    let before: Vec<String> = {
        let conn = db.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT analyzed_at FROM analysis ORDER BY photo_id").unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    };
    let s2 = run_analysis(&db, noop_progress(), cancel.clone());
    assert_eq!(s2.analyzed, 0, "re-run must be a no-op: {:?}", s2);
    assert_eq!(s2.failed, 0);
    let after: Vec<String> = {
        let conn = db.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT analyzed_at FROM analysis ORDER BY photo_id").unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    };
    assert_eq!(before, after, "rows must be untouched by a no-op re-run");

    // "Touch" one file's mtime (std has no mtime-setter; mtime storage is
    // second-granular, so a 1.1 s pause + rewrite guarantees a new mtime),
    // re-scan (refreshes photos.file_mtime), then analyze: exactly that
    // file is re-measured.
    let target = shoot.join("DARK.png");
    std::thread::sleep(Duration::from_millis(1_100));
    solid_png(&target, 320, 240, [10, 10, 10]);
    scanner::run_scan(&shoot, &db, &mut |_| {}, &cancel).unwrap();
    let s3 = run_analysis(&db, noop_progress(), cancel.clone());
    assert_eq!(s3.analyzed, 1, "only the touched file re-analyzes: {:?}", s3);
    assert_eq!(s3.failed, 0);

    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn analysis_reports_missing_files_friendly() {
    let (base, db_path, shoot) = setup("missing");
    solid_png(&shoot.join("A.png"), 64, 64, [120, 130, 140]);
    solid_png(&shoot.join("B.png"), 64, 64, [40, 60, 200]);

    let db = Arc::new(Db::open(&db_path).unwrap());
    db.migrate().unwrap();
    let ok = Arc::new(AtomicBool::new(false));
    scanner::run_scan(&shoot, &db, &mut |_| {}, &ok).unwrap();

    std::fs::remove_file(shoot.join("A.png")).unwrap();

    let s = run_analysis(&db, noop_progress(), ok);
    assert_eq!(s.analyzed, 1, "the surviving file is measured: {:?}", s);
    assert_eq!(s.failed, 1);
    assert_eq!(s.errors.len(), 1);
    assert!(
        s.errors[0].contains("A.png"),
        "friendly error names the file: {:?}",
        s.errors
    );
    assert!(s.errors[0].to_lowercase().contains("not found"));

    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn analysis_stops_on_cancel() {
    let (base, db_path, shoot) = setup("cancel");
    build_shoot(&shoot);

    let db = Arc::new(Db::open(&db_path).unwrap());
    db.migrate().unwrap();
    scanner::run_scan(&shoot, &db, &mut |_| {}, &AtomicBool::new(false))
        .unwrap();

    let cancel = Arc::new(AtomicBool::new(true)); // stopped before the run starts
    let s = run_analysis(&db, noop_progress(), cancel);
    assert_eq!(s.analyzed, 0, "nothing measured after cancel: {:?}", s);
    assert!(s.cancelled, "summary marks the run cancelled");
    assert!(s.errors.is_empty());

    // And the slot is honest: a follow-up run without cancel measures all.
    let s2 = run_analysis(&db, noop_progress(), Arc::new(AtomicBool::new(false)));
    assert_eq!(s2.analyzed, 4);
    assert!(!s2.cancelled);

    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_dir_all(&base);
}
