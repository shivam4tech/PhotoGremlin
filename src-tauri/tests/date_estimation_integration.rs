//! Integration test: date estimation through the real pipeline (Sprint 12).
//!
//! Real files on disk → `run_scan` → `run_metadata` → assert the stored
//! capture datetimes and their labelled provenance. Covers: camera-roll
//! filename dates, day-precision from the loose scan, mtime fallback for
//! unparseable names (the Unsplash case), and EXIF-beats-estimate.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

use photogremlin_lib::database::Db;
use photogremlin_lib::events::ProgressPayload;
use photogremlin_lib::metadata;
use photogremlin_lib::scanner;

fn plain_jpeg(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(w, h, image::Rgb([120, 90, 60]));
    let mut jpeg: Vec<u8> = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
        .encode(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    jpeg
}

/// JPEG with a tiny EXIF segment (DateTimeOriginal + Make), spliced after
/// the SOI marker the way camera files carry it.
fn jpeg_with_exif(w: u32, h: u32) -> Vec<u8> {
    use exif::{experimental::Writer, Field, In, Tag, Value};
    let fields = vec![
        Field {
            tag: Tag::Make,
            ifd_num: In::PRIMARY,
            value: Value::Ascii(vec![b"CamCo".to_vec()]),
        },
        Field {
            tag: Tag::DateTimeOriginal,
            ifd_num: In::PRIMARY,
            value: Value::Ascii(vec![b"2020:05:05 08:15:00".to_vec()]),
        },
    ];
    let mut writer = Writer::new();
    for f in &fields {
        writer.push_field(f);
    }
    let mut tiff: Vec<u8> = Vec::new();
    writer
        .write(&mut std::io::Cursor::new(&mut tiff), true)
        .unwrap();
    let mut exif_bytes = Vec::from("Exif\0\0".as_bytes());
    exif_bytes.extend(tiff);

    let mut jpeg = plain_jpeg(w, h);
    let mut out = jpeg.split_off(2);
    let len = (exif_bytes.len() + 2) as u16;
    let mut head = vec![0xFFu8, 0xE1, (len >> 8) as u8, (len & 0xff) as u8];
    head.extend_from_slice(&exif_bytes);
    let mut result = vec![0xFFu8, 0xD8u8];
    result.extend_from_slice(&head);
    result.append(&mut out);
    result
}

struct Env {
    root: PathBuf,
    db: Arc<Db>,
}

impl Env {
    fn new() -> Env {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("pg_date_est_{n}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let db_path = root.join("database.sqlite");
        let db = Db::open(&db_path).unwrap();
        db.migrate().unwrap();
        Env {
            root,
            db: Arc::new(db),
        }
    }

    fn add(&self, name: &str, bytes: &[u8], mtime: Option<&str>) {
        let path = self.root.join(name);
        std::fs::write(&path, bytes).unwrap();
        if let Some(iso) = mtime {
            let mtime = chrono::DateTime::parse_from_rfc3339(iso)
                .unwrap()
                .with_timezone(&chrono::Utc);
            let t: SystemTime = mtime.into();
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(t)
                .unwrap();
        }
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn run_pipeline(e: &Env) {
    let cancel = AtomicBool::new(false);
    scanner::run_scan(&e.root, &e.db, &mut |_: ProgressPayload| {}, &cancel).expect("scan");

    metadata::run_metadata(
        e.db.clone(),
        Arc::new(|_: ProgressPayload| {}),
        Arc::new(AtomicBool::new(false)),
    )
    .expect("metadata pass");
    // The command derives session shoot periods after the pass; mirror it.
    e.db.refresh_all_sessions_times().expect("session refresh");
}

fn photo_by_name(e: &Env, name: &str) -> photogremlin_lib::database::PhotoFull {
    let (photos, _total) = e
        .db
        .photos_where("", vec![], 0, 100)
        .expect("list photos");
    let p = photos
        .iter()
        .find(|p| p.filename == name)
        .unwrap_or_else(|| panic!("photo {name} not indexed"));
    e.db.get_photo_full(p.id).expect("photo full")
}

#[test]
fn capture_dates_are_estimated_and_labelled_through_the_pipeline() {
    let e = Env::new();
    // Camera-roll name → filename-source estimate, exact time.
    e.add("IMG_20250101_143022.jpg", &plain_jpeg(64, 64), None);
    // Generic loose pattern.
    e.add("holiday_20220605_123000.jpg", &plain_jpeg(32, 32), None);
    // Unparseable name (Unsplash-style) → stored mtime, labelled 'mtime'.
    e.add("U6KmF4RpgiU.jpg", &plain_jpeg(50, 40), Some("2026-08-17T12:00:00Z"));
    // Nikon-style name, old mtime.
    e.add("DSC_1234.jpg", &plain_jpeg(40, 40), Some("2024-06-01T10:00:00Z"));
    // EXIF camera + EXIF date present: EXIF wins over the filename pattern.
    e.add("IMG_20240101_000000.jpg", &jpeg_with_exif(32, 32), None);

    run_pipeline(&e);

    let img = photo_by_name(&e, "IMG_20250101_143022.jpg");
    assert_eq!(img.capture_datetime.as_deref(), Some("2025-01-01T14:30:22Z"));
    assert_eq!(img.capture_datetime_source.as_deref(), Some("filename"));
    assert_eq!(img.metadata_source, "filename");

    let holiday = photo_by_name(&e, "holiday_20220605_123000.jpg");
    assert_eq!(holiday.capture_datetime.as_deref(), Some("2022-06-05T12:30:00Z"));
    assert_eq!(holiday.capture_datetime_source.as_deref(), Some("filename"));

    let unsplash = photo_by_name(&e, "U6KmF4RpgiU.jpg");
    assert_eq!(
        unsplash.capture_datetime.as_deref(),
        Some("2026-08-17T12:00:00Z")
    );
    assert_eq!(unsplash.capture_datetime_source.as_deref(), Some("mtime"));
    assert_eq!(unsplash.metadata_source, "mtime");

    let dsc = photo_by_name(&e, "DSC_1234.jpg");
    assert_eq!(dsc.capture_datetime.as_deref(), Some("2024-06-01T10:00:00Z"));
    assert_eq!(dsc.capture_datetime_source.as_deref(), Some("mtime"));

    // EXIF dominates: real date + real camera; both labelled EXIF, and no
    // estimate can ever override a present EXIF date.
    let with_exif = photo_by_name(&e, "IMG_20240101_000000.jpg");
    assert_eq!(with_exif.capture_datetime.as_deref(), Some("2020-05-05T08:15:00Z"));
    assert_eq!(with_exif.capture_datetime_source.as_deref(), Some("exif"));
    assert_eq!(with_exif.camera_make.as_deref(), Some("CamCo"));
    assert_eq!(with_exif.metadata_source, "exif");
}

#[test]
fn session_periods_benefit_from_estimated_dates() {
    let e = Env::new();
    // Only unparseable (mtime-dated) photos in this shoot.
    e.add("R0000123.JPG", &plain_jpeg(30, 30), Some("2025-07-01T09:00:00Z"));
    e.add("R0000124.JPG", &plain_jpeg(30, 30), Some("2025-07-01T17:30:00Z"));

    run_pipeline(&e);

    // Sessions derive start/end from COALESCE(capture_datetime, indexed_at);
    // with estimates filling capture_datetime, the shoot period is real.
    let sessions = e.db.list_sessions().expect("sessions");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].start_time, Some("2025-07-01T09:00:00Z".to_string()));
    assert_eq!(sessions[0].end_time, Some("2025-07-01T17:30:00Z".to_string()));
}