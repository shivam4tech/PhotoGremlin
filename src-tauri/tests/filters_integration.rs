//! Integration test: seeded DB → filter engine → verify rows.
//!
//! The filter engine is pure (unit-tested in `filters`), so this test proves
//! the full path: parse JSON → build parameterized WHERE → `photos_where`
//! against the real schema with a `LEFT JOIN analysis`, including the
//! semantics of unanalyzed photos (flags NULL → no flag match), AND
//! composition, boundaries, and pagination.

use photogremlin_lib::analysis::metrics::Metrics;
use photogremlin_lib::database::{Db, PhotoUpsert};
use photogremlin_lib::filters::SqlParam;
use photogremlin_lib::filters::{self};
use photogremlin_lib::metadata::ExifRecord;

fn metrics(
    sharpness: f64,
    brightness: f64,
    mono: bool,
    dark: bool,
    bright: bool,
) -> Metrics {
    Metrics {
        sharpness,
        brightness,
        contrast: 50.0,
        saturation: if mono { 2.0 } else { 60.0 },
        highlight_clipping: 0.5,
        shadow_clipping: 0.2,
        is_monochrome: mono,
        is_dark: dark,
        is_bright: bright,
    }
}

fn upsert(db: &Db, session_id: i64, filename: &str, w: i64, h: i64) -> i64 {
    db.upsert_photo(&PhotoUpsert {
        path: format!("/shoot/{filename}"),
        filename: filename.into(),
        extension: "jpg".into(),
        size_bytes: Some(1234),
        width: Some(w),
        height: Some(h),
        orientation: None,
        session_id: Some(session_id),
        file_mtime: Some("2026-08-17T00:00:00Z".into()),
    })
    .unwrap()
}

fn exif(
    orientation: &str,
    camera_make: Option<&str>,
    camera_model: Option<&str>,
    lens: Option<&str>,
    focal_length: Option<f64>,
    iso: Option<i64>,
    aperture: Option<f64>,
    shutter_speed: Option<f64>,
    capture_datetime: Option<&str>,
) -> ExifRecord {
    ExifRecord {
        width: None,
        height: None,
        orientation: Some(orientation.to_string()),
        camera_make: camera_make.map(String::from),
        camera_model: camera_model.map(String::from),
        lens: lens.map(String::from),
        focal_length,
        iso,
        aperture,
        shutter_speed,
        capture_datetime: capture_datetime.map(String::from),
        gps_present: false,
    }
}

/// Seed six photos covering the interesting filter boundaries.
fn seed(db: &Db) {
    let session_id = db.upsert_session("Shoot", Some("/shoot")).unwrap();

    let p1 = upsert(db, session_id, "p1.jpg", 4000, 3000); // landscape, Sony, ISO 100
    let p2 = upsert(db, session_id, "p2.jpg", 3000, 4000); // portrait,  Canon, ISO 1600
    let p3 = upsert(db, session_id, "p3.jpg", 1024, 1024); // square,   no EXIF
    let p4 = upsert(db, session_id, "p4.jpg", 4000, 3000); // landscape, Sony, ISO 200, bright
    let p5 = upsert(db, session_id, "p5.jpg", 3000, 4000); // portrait, no EXIF, unanalyzed
    let p6 = upsert(db, session_id, "p6.jpg", 6000, 4000); // landscape, Nikon, just under 70

    db.upsert_exif(
        p1,
        &exif(
            "landscape",
            Some("Sony"),
            Some("A7"),
            Some("50mm F1.4"),
            Some(50.0),
            Some(100),
            Some(2.8),
            Some(0.004),
            Some("2026-06-15T10:00:00Z"),
        ),
    )
    .unwrap();
    db.upsert_exif(
        p2,
        &exif(
            "portrait",
            Some("Canon"),
            Some("R5"),
            None,
            Some(35.0),
            Some(1600),
            Some(4.0),
            Some(1.0 / 125.0),
            Some("2026-07-01T12:00:00Z"),
        ),
    )
    .unwrap();
    db.upsert_exif(p3, &exif("square", None, None, None, None, None, None, None, None))
        .unwrap();
    db.upsert_exif(
        p4,
        &exif(
            "landscape",
            Some("Sony"),
            Some("A7"),
            Some("85mm F1.8"),
            Some(85.0),
            Some(200),
            Some(1.8),
            Some(1.0 / 250.0),
            Some("2026-08-15T14:30:22Z"),
        ),
    )
    .unwrap();
    db.upsert_exif(p5, &exif("portrait", None, None, None, None, None, None, None, None))
        .unwrap();
    db.upsert_exif(
        p6,
        &exif(
            "landscape",
            Some("Nikon"),
            Some("Z6"),
            None,
            Some(24.0),
            Some(800),
            Some(5.6),
            Some(1.0 / 60.0),
            Some("2026-01-05T08:00:00Z"),
        ),
    )
    .unwrap();

    // Analysis on four of the six (p3 unanalyzed, p5 unanalyzed).
    db.upsert_analysis(
        p1,
        &metrics(85.0, 55.0, false, false, false),
        Some("2026-08-17T00:00:00Z"),
    )
    .unwrap();
    db.upsert_analysis(p2, &metrics(40.0, 80.0, true, false, false), Some("2026-08-17T00:00:00Z"))
        .unwrap();
    db.upsert_analysis(p4, &metrics(70.0, 66.0, false, false, true), Some("2026-08-17T00:00:00Z"))
        .unwrap();
    db.upsert_analysis(p6, &metrics(69.999, 40.0, false, false, false), Some("2026-08-17T00:00:00Z"))
        .unwrap();
}

/// Run a filter JSON through the real query path; return (filenames, total).
fn run(db: &Db, filter_json: &str, offset: i64, limit: i64) -> (Vec<String>, i64) {
    let filter = filters::parse_filter(filter_json).unwrap();
    let (where_sql, params) = filters::build_where(&filter).unwrap();
    let (photos, total) = db.photos_where(&where_sql, params, offset, limit).unwrap();
    (
        photos.iter().map(|p| p.filename.clone()).collect(),
        total,
    )
}

/// Each test gets its own database file (tests run in parallel threads).
fn db(label: &str) -> Db {
    let path = std::env::temp_dir().join(format!(
        "pg_filters_it_{label}_{}.sqlite",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let db = Db::open(&path).unwrap();
    db.migrate().unwrap();
    db
}

#[test]
fn empty_filter_returns_everything_paginated() {
    let db = db("empty");
    seed(&db);
    let (files, total) = run(&db, "", 0, 10);
    assert_eq!(total, 6);
    assert_eq!(files.len(), 6);
    // Ordered by capture datetime (NULLs last): Jan, Jun, Jul, Aug, then nulls.
    assert_eq!(files[0], "p6.jpg");
    assert_eq!(files[3], "p4.jpg");
    assert_eq!(files.len(), 6);
    let (page2, total2) = run(&db, "", 4, 10);
    assert_eq!(total2, 6);
    assert_eq!(page2.len(), 2);
}

#[test]
fn numeric_boundaries_and_and_composition() {
    let db = db("numeric");
    seed(&db);

    // sharpness >= 70: p1(85), p4(70 exact); p2(40) and p6(69.999) excluded
    // at the boundary, p3/p5 unanalyzed (NULL never matches).
    let (files, total) = run(
        &db,
        r#"{"operator":"AND","conditions":[{"field":"sharpness","operator":">=","value":70}]}"#,
        0,
        10,
    );
    assert_eq!(total, 2);
    assert!(files.contains(&"p1.jpg".to_string()));
    assert!(files.contains(&"p4.jpg".to_string()));

    // AND with orientation: portrait + sharp>=70 → nothing (p2 is 40).
    let (_files, total) = run(
        &db,
        r#"{"operator":"AND","conditions":[
            {"field":"sharpness","operator":">=","value":70},
            {"field":"orientation","operator":"=","value":"portrait"}]}"#,
        0,
        10,
    );
    assert_eq!(total, 0);

    // iso < 1600: p1(100), p4(200), p6(800); p2(1600) boundary-excluded;
    // p3/p5 have no EXIF (NULL never matches).
    let (files, total) = run(
        &db,
        r#"{"operator":"AND","conditions":[{"field":"iso","operator":"<","value":1600}]}"#,
        0,
        10,
    );
    assert_eq!(total, 3);
    assert!(files.contains(&"p1.jpg".to_string()));
    assert!(files.contains(&"p4.jpg".to_string()));
    assert!(files.contains(&"p6.jpg".to_string()));

    // between on capture datetime (string order == time order, UTC).
    let (files, total) = run(
        &db,
        r#"{"operator":"AND","conditions":[{"field":"capture_datetime","operator":"between","value":["2026-06-01","2026-08-31"]}]}"#,
        0,
        10,
    );
    assert_eq!(total, 3);
    assert!(files.contains(&"p1.jpg".to_string()));
    assert!(files.contains(&"p2.jpg".to_string()));
    assert!(files.contains(&"p4.jpg".to_string()));
}

#[test]
fn flags_include_unanalyzed_semantics() {
    let db = db("flags");
    seed(&db);

    // monochrome: only p2.
    let (files, total) = run(
        &db,
        r#"{"operator":"AND","conditions":[{"field":"monochrome","operator":"=","value":true}]}"#,
        0,
        10,
    );
    assert_eq!((total, files), (1, vec!["p2.jpg".to_string()]));

    // color (= not monochrome): the analyzed non-mono only — unanalyzed
    // p3/p5 have no flag and must NOT appear (unknown ≠ colored).
    let (files, total) = run(
        &db,
        r#"{"operator":"AND","conditions":[{"field":"color","operator":"=","value":true}]}"#,
        0,
        10,
    );
    assert_eq!(total, 3);
    assert!(files.contains(&"p1.jpg".to_string()));
    assert!(files.contains(&"p4.jpg".to_string()));
    assert!(files.contains(&"p6.jpg".to_string()));
    assert!(!files.contains(&"p3.jpg".to_string()));
    assert!(!files.contains(&"p5.jpg".to_string()));

    // dark/bright flags: p4 is bright; nobody is dark here.
    let (_, total) = run(
        &db,
        r#"{"operator":"AND","conditions":[{"field":"bright","operator":"=","value":true}]}"#,
        0,
        10,
    );
    assert_eq!(total, 1);

    // AI flags are all false until the model sprints fill face/smile counts.
    let (_, total) = run(
        &db,
        r#"{"operator":"AND","conditions":[{"field":"faces_present","operator":"=","value":true}]}"#,
        0,
        10,
    );
    assert_eq!(total, 0);
}

#[test]
fn text_in_and_null_operators() {
    let db = db("text");
    seed(&db);

    // camera_model in [Sony A7, Canon R5] → p1, p2, p4.
    let (files, total) = run(
        &db,
        r#"{"operator":"AND","conditions":[{"field":"camera_model","operator":"in","value":["A7","R5"]}]}"#,
        0,
        10,
    );
    assert_eq!(total, 3);
    assert!(files.contains(&"p1.jpg".to_string()));
    assert!(files.contains(&"p2.jpg".to_string()));
    assert!(files.contains(&"p4.jpg".to_string()));

    // lens is-null → p2, p3, p5, p6 (p1/p4 have lenses).
    let (_files, total) = run(
        &db,
        r#"{"operator":"AND","conditions":[{"field":"lens","operator":"is-null","value":null}]}"#,
        0,
        10,
    );
    assert_eq!(total, 4);

    // camera_make = "Sony" AND iso >= 150 → p4 only.
    let (files, total) = run(
        &db,
        r#"{"operator":"AND","conditions":[
            {"field":"camera_make","operator":"=","value":"Sony"},
            {"field":"iso","operator":">=","value":150}]}"#,
        0,
        10,
    );
    assert_eq!((total, files), (1, vec!["p4.jpg".to_string()]));
}

#[test]
fn invalid_filters_fail_friendly() {
    let cases = [
        // unknown field
        r#"{"operator":"AND","conditions":[{"field":"vibes","operator":"=","value":1}]}"#,
        // bad value type
        r#"{"operator":"AND","conditions":[{"field":"iso","operator":"=","value":"fast"}]}"#,
        // operator the kind does not support
        r#"{"operator":"AND","conditions":[{"field":"dark","operator":"in","value":true}]}"#,
        // garbage JSON
        "{not json",
        // top-level operator outside the v0.1 set
        r#"{"operator":"OR","conditions":[]}"#,
    ];
    for json in &cases {
        let outcome = filters::parse_filter(json).and_then(|f| filters::build_where(&f).map(|_| ()));
        assert!(
            outcome.is_err(),
            "expected a friendly error for: {json} (got {outcome:?})"
        );
    }
}

#[test]
fn param_binding_is_injection_safe() {
    let db = db("invalid");
    seed(&db);
    // A value that looks like SQL must be bound, not spliced.
    let (files, total) = run(
        &db,
        r#"{"operator":"AND","conditions":[{"field":"camera_make","operator":"=","value":"Sony'; DROP TABLE photos; --"}]}"#,
        0,
        10,
    );
    assert_eq!(total, 0);
    assert_eq!(files, Vec::<String>::new());
    // The table is still alive.
    let (_, total) = run(&db, "", 0, 10);
    assert_eq!(total, 6);
}

#[test]
fn sql_param_bindings_survive_the_real_query() {
    let db = db("injection");
    seed(&db);
    // Every param kind (Int/Real/Text/Bool) through one real query:
    let filter = filters::parse_filter(
        r#"{"operator":"AND","conditions":[
            {"field":"iso","operator":">=","value":100},
            {"field":"aperture","operator":"<=","value":5.0},
            {"field":"camera_make","operator":"=","value":"Sony"},
            {"field":"bright","operator":"=","value":true}]}"#,
    )
    .unwrap();
    let (where_sql, params) = filters::build_where(&filter).unwrap();
    let kinds: Vec<String> = params
        .iter()
        .map(|p| match p {
            SqlParam::Int(_) => "int".into(),
            SqlParam::Real(_) => "real".into(),
            SqlParam::Text(_) => "text".into(),
            SqlParam::Bool(_) => "bool".into(),
        })
        .collect();
    assert_eq!(kinds, vec!["int", "real", "text", "bool"]);
    let (photos, total) = db.photos_where(&where_sql, params, 0, 10).unwrap();
    assert_eq!(total, 1);
    assert_eq!(photos[0].filename, "p4.jpg");
}
