//! Integration test: RAW previews (Sprint 15) through the full thumbnail
//! path — a real DB row + real file, exactly like the UI hits it. The DNG
//! is synthesized at runtime (AGENTS.md rule 16: no real-file fixtures);
//! garbage raws must degrade to the placeholder error, never crash.

use std::path::Path;

use photogremlin_lib::database::{Db, PhotoUpsert};
use photogremlin_lib::thumbnailer::{ThumbData, ThumbKind, ThumbService};

fn upsert(db: &Db, root: &Path, filename: &str) -> i64 {
    db.upsert_photo(&PhotoUpsert {
        path: root.join(filename).display().to_string(),
        filename: filename.to_string(),
        extension: filename.rsplit('.').next().unwrap().to_string(),
        size_bytes: Some(1),
        width: Some(16),
        height: Some(16),
        orientation: None,
        session_id: None,
        file_mtime: None,
    })
    .unwrap()
}

fn synthetic_dng_bytes() -> Vec<u8> {
    // Reuse the lib's synthetic fixture (16×16 RGGB, uncompressed, 16-bit):
    // no real-file fixtures in the repo (AGENTS.md rule 16).
    photogremlin_lib::decode::synthetic_dng_bytes()
}

#[tokio::test]
async fn dng_previews_and_garbage_raw_keeps_placeholder() {
    let root = std::env::temp_dir().join(format!("pg_rawprev_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("IMG_0001.DNG"), synthetic_dng_bytes()).unwrap();
    std::fs::write(root.join("IMG_0002.CR2"), b"definitely not a camera raw").unwrap();

    let db = Db::open(&root.join("t.sqlite")).unwrap();
    db.migrate().unwrap();
    let dng_id = upsert(&db, &root, "IMG_0001.DNG");
    let cr2_id = upsert(&db, &root, "IMG_0002.CR2");

    let svc = ThumbService::new(root.join("cache"));

    // A decodable DNG produces a real preview.
    let dng: ThumbData = svc.get(&db, dng_id, ThumbKind::Grid).await.unwrap();
    assert!(!dng.data_url.is_empty());
    assert!(dng.width > 0 && dng.height > 0);

    // An undecodable raw degrades to the placeholder error (same contract
    // as HEIC tiles), and must NOT leave a corrupt cache entry behind.
    let err = svc.get(&db, cr2_id, ThumbKind::Grid).await.unwrap_err().to_string();
    assert!(err.contains("Unsupported"), "got: {err}");
    let cache_files = std::fs::read_dir(&svc.cache_dir())
        .unwrap()
        .filter_map(|e| e.ok())
        .count();
    assert_eq!(cache_files, 1, "only the DNG thumbnail may be cached");

    let _ = std::fs::remove_dir_all(&root);
}