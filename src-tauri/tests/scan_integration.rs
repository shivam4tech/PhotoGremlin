//! Integration test: synthetic folder -> scan -> verify -> re-scan idempotence.
//!
//! Covers the Sprint 2 acceptance path without touching real photos.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use image::ImageBuffer;
use photogremlin_lib::database::Db;
use photogremlin_lib::events::ProgressPayload;
use photogremlin_lib::scanner::{self, FileClass};

fn write_jpg(path: &Path, w: u32, h: u32) {
    // JPEG has no alpha: encode as RGB.
    let img: ImageBuffer<image::Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(w, h, |x, y| {
        image::Rgb([x as u8, y as u8, 128])
    });
    img.save(path).expect("save jpg");
}

/// (base, db_path, shoot_folder). Each test gets its own base so parallel
/// tests never wipe each other's fixtures. Cleanup is the caller's job.
fn setup(label: &str) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let base = std::env::temp_dir().join(format!(
        "pg_scan_it_{}_{}",
        std::process::id(),
        label
    ));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    (base.clone(), base.join("db.sqlite"), base.join("Wedding_Test"))
}

fn count_photos(db: &Db) -> i64 {
    let conn = db.lock().unwrap();
    conn.query_row("SELECT COUNT(*) FROM photos", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn scan_indexes_a_synthetic_shoot_and_is_idempotent() {
    let (base, db_path, shoot) = setup("main");
    // Build the folder: 2 jpgs (one nested), 1 png, 1 "RAW", 1 non-photo,
    // plus a hidden dir that must be skipped.
    std::fs::create_dir_all(shoot.join("nested/deep")).unwrap();
    write_jpg(&shoot.join("IMG_0001.JPG"), 640, 480);
    write_jpg(&shoot.join("nested/deep/IMG_0003.jpg"), 800, 600);
    {
        let img: ImageBuffer<image::Rgb<u8>, Vec<u8>> =
            ImageBuffer::from_fn(320, 240, |x, y| image::Rgb([(x as u8), (y as u8), 200]));
        img.save(shoot.join("IMG_0002.png")).unwrap();
    }
    std::fs::write(shoot.join("RAW_0001.CR3"), b"fake-raw-bytes").unwrap();
    std::fs::write(shoot.join("notes.txt"), b"do not index me").unwrap();
    std::fs::create_dir_all(shoot.join(".hidden")).unwrap();
    write_jpg(&shoot.join(".hidden/secret.jpg"), 8, 8);

    let db = Db::open(&db_path).unwrap();
    db.migrate().unwrap();

    let cancel = AtomicBool::new(false);
    let mut progress_log: Vec<(String, usize, usize)> = Vec::new();
    let summary = scanner::run_scan(
        &shoot,
        &db,
        &mut |p: ProgressPayload| {
            progress_log.push((p.stage.clone(), p.done, p.total));
        },
        &cancel,
    )
    .unwrap();

    // 6 files on disk that walkdir sees: 4 photo-ish + notes.txt;
    // secret.jpg lives in a hidden dir and is skipped entirely.
    assert_eq!(summary.total_files, 5);
    assert_eq!(summary.indexed, 4);
    assert_eq!(summary.ignored, 1); // notes.txt
    assert!(!summary.cancelled);
    assert!(summary.errors.is_empty(), "unexpected errors: {:?}", summary.errors);
    // A 5-file synthetic scan must be fast (sanity ceiling, not a floor).
    assert!(summary.elapsed_ms < 60_000);
    assert_eq!(summary.session_name, "Wedding_Test");

    let session = db.session_by_id(summary.session_id).unwrap().unwrap();
    assert_eq!(session.name, "Wedding_Test");
    assert_eq!(session.photo_count, 4);
    assert_eq!(session.root_path.as_deref(), Some(shoot.to_str().unwrap()));

    // Decodables got real dimensions + orientation; RAW got neither.
    let conn = db.lock().unwrap();
    let (w, h, o): (i64, i64, String) = conn
        .query_row(
            "SELECT width, height, orientation FROM photos WHERE filename = 'IMG_0001.JPG'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((w, h), (640, 480));
    assert_eq!(o, "landscape");

    let (cw, co): (Option<i64>, Option<String>) = conn
        .query_row(
            "SELECT width, orientation FROM photos WHERE filename = 'RAW_0001.CR3'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(cw, None);
    assert_eq!(co, None);

    // Hidden file must not have been indexed.
    let hidden_idx: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM photos WHERE filename = 'secret.jpg'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(hidden_idx, 0);
    drop(conn);

    assert_eq!(count_photos(&db), 4);

    // Progress contract: discovered, indexed sweeps, and a final done.
    assert!(progress_log.iter().any(|(s, _, _)| s == "discovering"));
    assert!(progress_log.iter().any(|(s, _, _)| s == "indexing"));
    let (last_stage, last_done, last_total) = progress_log.last().cloned().unwrap();
    assert_eq!(last_stage, "done");
    assert_eq!(last_total, 4);
    assert_eq!(last_done, 4);

    // Re-scan: idempotent, same session, no duplicates.
    let summary2 = scanner::run_scan(&shoot, &db, &mut |_| {}, &cancel).unwrap();
    assert_eq!(summary2.session_id, summary.session_id);
    assert_eq!(summary2.indexed, 4);
    assert_eq!(summary2.errors.len(), summary.errors.len());
    assert_eq!(count_photos(&db), 4);

    let sessions: i64 = {
        let conn = db.lock().unwrap();
        conn.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0)).unwrap()
    };
    assert_eq!(sessions, 1);

    // Nonexistent folder -> friendly error, not a panic.
    assert!(scanner::run_scan(&base.join("does_not_exist"), &db, &mut |_| {}, &cancel).is_err());

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn cancel_before_start_reports_cancelled() {
    let (base, db_path, shoot) = setup("cancel");
    std::fs::create_dir_all(&shoot).unwrap();
    for i in 0..10 {
        write_jpg(&shoot.join(format!("IMG_{i:04}.jpg")), 16, 16);
    }

    let db = Db::open(&db_path).unwrap();
    db.migrate().unwrap();

    let cancel = AtomicBool::new(true); // pre-cancelled: nothing may be indexed
    let summary = scanner::run_scan(&shoot, &db, &mut |_| {}, &cancel).unwrap();
    assert!(summary.cancelled);
    assert_eq!(summary.indexed, 0);
    assert_eq!(count_photos(&db), 0);

    let _ = std::fs::remove_dir_all(&base);
}

/// Spec §Sprint 2 acceptance: "a real folder containing thousands of
/// photographs can be scanned and indexed." 1,000 tiny files keep this in
/// seconds; the pipeline is the same one a 10k+ shoot would run.
#[test]
fn scan_indexes_thousands_of_files() {
    let (base, db_path, shoot) = setup("thousands");
    std::fs::create_dir_all(&shoot).unwrap();
    const N: u32 = 1000;
    for i in 0..N {
        write_jpg(&shoot.join(format!("IMG_{i:04}.jpg")), 8, 8);
    }

    let db = Db::open(&db_path).unwrap();
    db.migrate().unwrap();

    let started = std::time::Instant::now();
    let cancel = AtomicBool::new(false);
    let summary = scanner::run_scan(&shoot, &db, &mut |_| {}, &cancel).unwrap();
    let wall = started.elapsed().as_secs_f64();

    assert_eq!(summary.indexed, N as usize);
    assert_eq!(summary.ignored, 0);
    assert!(summary.errors.is_empty(), "errors: {:?}", summary.errors);
    assert_eq!(count_photos(&db), N as i64);

    let session = db.session_by_id(summary.session_id).unwrap().unwrap();
    assert_eq!(session.photo_count, N as i64);
    // A 1000-file scan of tiny images must be comfortably fast.
    assert!(
        wall < 120.0,
        "scan took {wall}s — regression (indexed {N} files)"
    );

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn classification_covers_all_spec_listed_formats() {
    // Spec §12: CR2 CR3 NEF ARW RAF DNG ORF RW2 (RAW);
    // JPG JPEG PNG WebP TIFF TIF (decodable).
    for raw in ["cr2", "cr3", "nef", "arw", "raf", "dng", "orf", "rw2"] {
        assert_eq!(scanner::classify_extension(raw), FileClass::Raw, "{raw}");
    }
    for dec in ["jpg", "jpeg", "png", "webp", "tif", "tiff"] {
        assert_eq!(scanner::classify_extension(dec), FileClass::Decodable, "{dec}");
    }
}
