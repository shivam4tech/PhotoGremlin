//! Integration test: folder → scan → metadata pass → verify EXIF landed.
//!
//! Covers the Sprint 5 acceptance path without real photos: the pass reads
//! EXIF out of decodable files, stores camera fields + presence-only GPS,
//! stamps `exif_at` so re-runs are no-ops, treats "no EXIF" as success, and
//! fails missing files with a friendly error.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use exif::{experimental::Writer, Field, In, Rational, Tag, Value};
use photogremlin_lib::database::Db;
use photogremlin_lib::events::ProgressPayload;
use photogremlin_lib::metadata::{self, MetadataSummary};
use photogremlin_lib::scanner;

fn plain_jpeg(w: u32, h: u32) -> Vec<u8> {
    let img = image::ImageBuffer::from_pixel(w, h, image::Rgb([120, 90, 60]));
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

fn encode_exif_fields(fields: &[Field]) -> Vec<u8> {
    let mut writer = Writer::new();
    for f in fields {
        writer.push_field(f);
    }
    let mut tiff: Vec<u8> = Vec::new();
    writer.write(&mut std::io::Cursor::new(&mut tiff), true).unwrap();
    let mut out = Vec::from("Exif\0\0".as_bytes());
    out.extend(tiff);
    out
}

/// JPEG + APP1 EXIF spliced after SOI (the way camera files carry it).
fn jpeg_with_exif(w: u32, h: u32, fields: &[Field]) -> Vec<u8> {
    let payload = encode_exif_fields(fields);
    let mut jpeg = plain_jpeg(w, h);
    let mut out = jpeg.split_off(2);
    let len = (payload.len() + 2) as u16;
    let mut head = vec![0xFFu8, 0xE1, (len >> 8) as u8, (len & 0xff) as u8];
    head.extend_from_slice(&payload);
    let mut result = vec![0xFFu8, 0xD8u8];
    result.extend_from_slice(&head);
    result.append(&mut out);
    result
}

fn full_camera_fields() -> Vec<Field> {
    vec![
        Field {
            tag: Tag::Make,
            ifd_num: In::PRIMARY,
            value: Value::Ascii(vec![b"GremCam".to_vec()]),
        },
        Field {
            tag: Tag::Model,
            ifd_num: In::PRIMARY,
            value: Value::Ascii(vec![b"Gr-33".to_vec()]),
        },
        Field {
            tag: Tag::LensModel,
            ifd_num: In::PRIMARY,
            value: Value::Ascii(vec![b"Gremlin 50mm f/1.4".to_vec()]),
        },
        Field {
            tag: Tag::FNumber,
            ifd_num: In::PRIMARY,
            value: Value::Rational(vec![Rational { num: 14, denom: 5 }]),
        },
        Field {
            tag: Tag::ExposureTime,
            ifd_num: In::PRIMARY,
            value: Value::Rational(vec![Rational { num: 1, denom: 250 }]),
        },
        Field {
            tag: Tag::ISOSpeed,
            ifd_num: In::PRIMARY,
            value: Value::Short(vec![400]),
        },
        Field {
            tag: Tag::FocalLength,
            ifd_num: In::PRIMARY,
            value: Value::Rational(vec![Rational { num: 5000, denom: 1 }]),
        },
        Field {
            tag: Tag::DateTimeOriginal,
            ifd_num: In::PRIMARY,
            value: Value::Ascii(vec![b"2026:08:15 14:30:22".to_vec()]),
        },
        Field {
            tag: Tag::GPSLatitudeRef,
            ifd_num: In::PRIMARY,
            value: Value::Ascii(vec![b"N".to_vec()]),
        },
        Field {
            tag: Tag::GPSLatitude,
            ifd_num: In::PRIMARY,
            value: Value::Rational(vec![Rational { num: 52, denom: 1 }]),
        },
    ]
}

fn setup(label: &str) -> (PathBuf, PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!("pg_metadata_it_{label}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let shoot = base.join("Shoot_Metadata");
    std::fs::create_dir_all(&shoot).unwrap();
    (base.clone(), base.join("db.sqlite"), shoot)
}

fn noop_progress() -> Arc<dyn Fn(ProgressPayload) + Send + Sync> {
    Arc::new(|_p: ProgressPayload| {})
}

/// Raw row readout for assertions (test-only escape hatch over the public
/// lock; the app itself only uses purpose-built queries).
fn read_row(db: &Db, file: &str) -> (i64, Option<String>, Option<i64>, Option<f64>, Option<f64>, Option<f64>, Option<String>, Option<String>, i64, Option<String>) {
    let conn = db.lock().unwrap();
    conn.query_row(
        "SELECT id, camera_model, iso, aperture, shutter_speed, focal_length,
                capture_datetime, exif_at, gps_present, orientation
         FROM photos WHERE filename = ?1",
        [file],
        |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
            ))
        },
    )
    .unwrap()
}

#[test]
fn pass_reads_exif_stamps_and_is_incremental() {
    let (_base, db_path, shoot) = setup("main");
    let db = Arc::new(Db::open(&db_path).unwrap());
    db.migrate().unwrap();

    let with_exif = shoot.join("DSC0001.jpg");
    let without_exif = shoot.join("DSC0002.jpg");
    std::fs::write(&with_exif, jpeg_with_exif(6400, 4266, &full_camera_fields())).unwrap();
    std::fs::write(&without_exif, plain_jpeg(800, 600)).unwrap();

    let mut progress = |p: ProgressPayload| {
        let _ = p;
    };
    let cancel = AtomicBool::new(false);
    let scan = scanner::run_scan(&shoot, &db, &mut progress, &cancel).unwrap();
    assert_eq!(scan.indexed, 2, "scan should index both jpgs");

    // Before the pass: queue holds both, nothing stamped.
    assert_eq!(db.exif_queue().unwrap().len(), 2);

    let sum = metadata::run_metadata(db.clone(), noop_progress(), Arc::new(cancel))
        .unwrap();
    assert_eq!(sum.processed, 2, "both files processed: {:?}", sum);
    assert_eq!(sum.failed, 0, "no failures expected: {:?}", sum);
    assert!(!sum.cancelled);

    // After the pass: queue drained (stamped) and the re-run is a no-op.
    assert!(db.exif_queue().unwrap().is_empty(), "queue must drain");
    let again = metadata::run_metadata(db.clone(), noop_progress(), Arc::new(AtomicBool::new(false)))
        .unwrap();
    assert_eq!(again.processed, 0, "re-run must be a no-op");

    // EXIF fields landed verbatim; orientation derived from dims.
    let (id, model, iso, aperture, shutter, focal, dt, exif_at, gps, orientation) =
        read_row(&db, "DSC0001.jpg");
    assert_eq!(model.as_deref(), Some("Gr-33"));
    assert_eq!(iso, Some(400));
    assert!((aperture.unwrap() - 2.8).abs() < 1e-9);
    assert!((shutter.unwrap() - 1.0 / 250.0).abs() < 1e-9);
    assert!((focal.unwrap() - 50.0).abs() < 1e-9);
    assert_eq!(dt.as_deref(), Some("2026-08-15T14:30:22Z"));
    assert!(exif_at.is_some(), "exif_at must be stamped");
    assert_eq!(gps, 1, "GPS presence only — no coordinates in the DB");
    assert_eq!(orientation.as_deref(), Some("landscape"));
    // Public record also carries the fields.
    let full = db.get_photo_full(id).unwrap();
    assert_eq!(full.camera_model.as_deref(), Some("Gr-33"));
    assert!(full.gps_present);
    assert!(full.lens.as_deref() == Some("Gremlin 50mm f/1.4"));

    // File without EXIF: processed (stamped) but camera fields stay NULL.
    let (_, model2, iso2, _, _, _, _, exif_at2, gps2, orientation2) = read_row(&db, "DSC0002.jpg");
    assert_eq!(model2, None);
    assert_eq!(iso2, None);
    assert!(exif_at2.is_some(), "no-EXIF files are still stamped processed");
    assert_eq!(gps2, 0);
    assert_eq!(orientation2.as_deref(), Some("landscape"));
}

#[test]
fn missing_file_is_a_friendly_failure() {
    let (_base, db_path, shoot) = setup("missing");
    let db = Arc::new(Db::open(&db_path).unwrap());
    db.migrate().unwrap();

    // Index a file, then delete it before the metadata pass runs.
    let gone = shoot.join("GONE.jpg");
    std::fs::write(&gone, plain_jpeg(100, 100)).unwrap();
    let mut progress = |p: ProgressPayload| {
        let _ = p;
    };
    let cancel = AtomicBool::new(false);
    let scan = scanner::run_scan(&shoot, &db, &mut progress, &cancel).unwrap();
    assert_eq!(scan.indexed, 1);
    std::fs::remove_file(&gone).unwrap();

    let sum = metadata::run_metadata(db.clone(), noop_progress(), Arc::new(AtomicBool::new(false)))
        .unwrap();
    assert_eq!(sum.processed, 0);
    assert_eq!(sum.failed, 1);
    assert!(
        sum.errors.first().map(|s| s.contains("GONE.jpg")).unwrap_or(false),
        "friendly error should name the file: {:?}",
        sum.errors
    );
}

#[test]
fn metadata_pass_only_reads_the_open_project() {
    let (_base, db_path, shoot) = setup("project_scope");
    let other_shoot = shoot.parent().unwrap().join("Other_Shoot");
    std::fs::create_dir_all(&other_shoot).unwrap();
    let db = Arc::new(Db::open(&db_path).unwrap());
    db.migrate().unwrap();

    let first = shoot.join("FIRST.jpg");
    let second = other_shoot.join("SECOND.jpg");
    std::fs::write(&first, plain_jpeg(100, 100)).unwrap();
    std::fs::write(&second, plain_jpeg(100, 100)).unwrap();
    let mut progress = |_p: ProgressPayload| {};
    let cancel = AtomicBool::new(false);
    scanner::run_scan(&shoot, &db, &mut progress, &cancel).unwrap();
    scanner::run_scan(&other_shoot, &db, &mut progress, &cancel).unwrap();

    db.set_setting("active_folder", shoot.to_str().unwrap()).unwrap();
    assert_eq!(db.exif_queue().unwrap().len(), 1, "only the open project queues work");
    let summary = metadata::run_metadata(db.clone(), noop_progress(), Arc::new(AtomicBool::new(false)))
        .unwrap();
    assert_eq!(summary.processed, 1, "only the open project is read");
    assert!(read_row(&db, "FIRST.jpg").7.is_some());
    assert!(read_row(&db, "SECOND.jpg").7.is_none(), "other project remains pending");
}

#[test]
fn cancel_before_start_stops_the_pass() {
    let (_base, db_path, shoot) = setup("cancel");
    let db = Arc::new(Db::open(&db_path).unwrap());
    db.migrate().unwrap();

    for i in 0..12 {
        let p = shoot.join(format!("IMG_{:04}.jpg", i));
        std::fs::write(&p, plain_jpeg(120, 90)).unwrap();
    }
    let mut progress = |p: ProgressPayload| {
        let _ = p;
    };
    let cancel = AtomicBool::new(false);
    let _ = scanner::run_scan(&shoot, &db, &mut progress, &cancel).unwrap();
    assert_eq!(db.exif_queue().unwrap().len(), 12);

    let already_cancelled = Arc::new(AtomicBool::new(true));
    let sum: MetadataSummary =
        metadata::run_metadata(db.clone(), noop_progress(), already_cancelled).unwrap();
    assert_eq!(sum.processed, 0);
    assert!(sum.cancelled, "pre-cancelled run must report cancelled");
    assert_eq!(db.exif_queue().unwrap().len(), 12, "nothing stamped on cancel");
}
