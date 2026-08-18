//! Integration test: contact sheets (Sprint 14) — real DB rows through the
//! sheet path (`sheet_photos`) into rendered PNG pages, plus the hard cap.

use std::sync::atomic::AtomicBool;

use photogremlin_lib::contact_sheet::{self, SheetOutcome};
use photogremlin_lib::database::{Db, PhotoUpsert};
use photogremlin_lib::events::ProgressPayload;

fn plain_jpeg(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(w, h, image::Rgb([90, 120, 160]));
    let mut jpeg: Vec<u8> = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85)
        .encode(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    jpeg
}

fn seed_rows(db: &Db, n: usize, root: &std::path::Path) -> Vec<i64> {
    (0..n)
        .map(|i| {
            db.upsert_photo(&PhotoUpsert {
                path: root.join(format!("IMG_2026{i:04}.jpg")).display().to_string(),
                filename: format!("IMG_2026{i:04}.jpg"),
                extension: "jpg".to_string(),
                size_bytes: Some(1),
                width: Some(8),
                height: Some(8),
                orientation: None,
                session_id: None,
                file_mtime: None,
            })
            .unwrap()
        })
        .collect()
}

#[test]
fn contact_sheet_flows_db_rows_into_png_pages() {
    let root = std::env::temp_dir().join(format!("pg_cs_db_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let db = Db::open(&root.join("database.sqlite")).unwrap();
    db.migrate().unwrap();
    let ids = seed_rows(&db, 13, &root);
    let out = root.join("sheets");
    std::fs::create_dir_all(&out).unwrap();

    // Give one photo a real capture date (caption shows it; others "—").
    db.upsert_exif(
        ids[0],
        &photogremlin_lib::metadata::exif::ExifRecord {
            capture_datetime: Some("2026-06-15T10:00:00Z".to_string()),
            ..Default::default()
        },
        None,
    )
    .unwrap();

    let mut photos = contact_sheet::sheet_photos(&db, &ids).unwrap();
    assert_eq!(photos.len(), 13);
    assert_eq!(photos[0].capture_datetime.as_deref(), Some("2026-06-15T10:00:00Z"));
    assert_eq!(photos[1].capture_datetime, None);

    // Stand in for thumbnails the async service would fetch.
    photos
        .iter_mut()
        .enumerate()
        .for_each(|(i, p)| {
            let bytes = if i % 2 == 0 { plain_jpeg(640, 480) } else { plain_jpeg(480, 640) };
            p.thumb = Some((bytes, 640, 480));
        });

    let outcome = contact_sheet::render_sheets(
        &mut photos,
        "June shoot",
        &out,
        &mut |_p: ProgressPayload| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    let SheetOutcome::Ok { pages: pages } = outcome else {
        panic!("expected ok, got cancelled");
    };

    assert_eq!(pages.len(), 2, "13 photos → 2 pages (12 per page)");
    assert!(pages[0].file_name().unwrap().to_string_lossy().ends_with("-p01.png"));
    let img = image::open(&pages[0]).unwrap();
    assert_eq!(img.width(), contact_sheet::PAGE_W);
    assert_eq!(img.height(), contact_sheet::PAGE_H);

    // Missing rows fail as a batch (ids include a deleted photo).
    db.delete_photos(vec![ids[5]]).unwrap();
    let err = contact_sheet::sheet_photos(&db, &ids).unwrap_err();
    assert!(err.to_string().contains("no longer in the library"), "{err}");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn contact_sheet_cap_is_enforced() {
    let root = std::env::temp_dir().join(format!("pg_cs_cap_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let db = Db::open(&root.join("database.sqlite")).unwrap();
    db.migrate().unwrap();
    let ids = seed_rows(&db, contact_sheet::MAX_SHEET_PHOTOS + 1, &root);

    let err = contact_sheet::sheet_photos(&db, &ids).unwrap_err();
    assert!(
        err.to_string().contains("at most"),
        "expected friendly cap message, got: {err}"
    );

    let _ = std::fs::remove_dir_all(&root);
}