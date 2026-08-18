//! Integration test: seeded DB → statistics engine → verify the aggregates.
//!
//! The binning and period resolution are pure (unit-tested), so this proves
//! the full path against the real schema: totals, analyzed-only averages,
//! shares (incl. honest `None`s), fixed-bin histograms, usage tables, the
//! monthly trend, scoped selection stats, session summary and side-by-side
//! comparison — plus the session start/end time refresh.

use chrono::{DateTime, TimeZone, Utc};
use photogremlin_lib::analysis::metrics::Metrics;
use photogremlin_lib::database::{Db, PhotoUpsert};
use photogremlin_lib::metadata::ExifRecord;
use photogremlin_lib::statistics::{
    compare_sessions, parse_period, period_stats, session_summary, Period,
};

/// Pinned "now" for period resolution: a Monday afternoon in August 2026.
fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 17, 12, 0, 0).unwrap()
}

fn metrics(sharpness: f64, brightness: f64, mono: bool) -> Metrics {
    Metrics {
        sharpness,
        brightness,
        contrast: 50.0,
        saturation: if mono { 2.0 } else { 60.0 },
        highlight_clipping: 0.5,
        shadow_clipping: 0.2,
        is_monochrome: mono,
        is_dark: false,
        is_bright: false,
    }
}

fn upsert(db: &Db, session_id: i64, dir: &str, filename: &str) -> i64 {
    db.upsert_photo(&PhotoUpsert {
        path: format!("/{dir}/{filename}"),
        filename: filename.into(),
        extension: "jpg".into(),
        size_bytes: Some(1234),
        width: Some(4000),
        height: Some(3000),
        orientation: None,
        session_id: Some(session_id),
        file_mtime: Some("2026-08-17T00:00:00Z".into()),
    })
    .unwrap()
}

fn exif(
    camera_make: Option<&str>,
    camera_model: Option<&str>,
    lens: Option<&str>,
    focal: Option<f64>,
    iso: Option<i64>,
    aperture: Option<f64>,
    shutter: Option<f64>,
    capture: Option<&str>,
) -> ExifRecord {
    ExifRecord {
        width: None,
        height: None,
        orientation: None,
        camera_make: camera_make.map(String::from),
        camera_model: camera_model.map(String::from),
        lens: lens.map(String::from),
        lens_make: None,
        software: None,
        focal_length: focal,
        iso,
        aperture,
        shutter_speed: shutter,
        capture_datetime: capture.map(String::from),
        gps_present: false,
    }
}

fn seed(db: &Db) -> (i64, i64) {
    let a = db.upsert_session("June shoot", Some("/shoot/a")).unwrap();
    let b = db.upsert_session("July shoot", Some("/shoot/b")).unwrap();

    let a1 = upsert(db, a, "shoot/a", "a1.jpg");
    let a2 = upsert(db, a, "shoot/a", "a2.jpg");
    let b1 = upsert(db, b, "shoot/b", "b1.jpg");
    let b2 = upsert(db, b, "shoot/b", "b2.jpg");

    db.upsert_exif(
        a1,
        &exif(Some("Sony"), Some("A7"), Some("50mm F1.4"), Some(50.0), Some(100), Some(2.8),
              Some(0.004), Some("2026-06-15T10:00:00Z")),
    )
    .unwrap();
    db.upsert_exif(
        a2,
        &exif(Some("Sony"), Some("A7"), Some("50mm F1.4"), Some(50.0), Some(400), Some(2.8),
              Some(0.004), Some("2026-06-20T15:00:00Z")),
    )
    .unwrap();
    db.upsert_exif(
        b1,
        &exif(Some("Canon"), Some("R5"), None, None, Some(1600), Some(4.0), Some(0.5),
              Some("2026-07-01T12:00:00Z")),
    )
    .unwrap();
    // b2 deliberately gets NO EXIF record (exif_at stays NULL).

    db.upsert_analysis(a1, &metrics(85.0, 55.0, false), None).unwrap();
    db.upsert_analysis(a2, &metrics(40.0, 80.0, true), None).unwrap();
    // b1 stays unanalyzed.
    db.upsert_analysis(b2, &metrics(60.0, 45.0, false), None).unwrap();

    // AI columns: one photo with face/smile data (faces yes, smile no);
    // everyone else stays NULL (no AI data at all).
    let conn = db.lock().unwrap();
    conn.execute(
        "UPDATE analysis SET face_count = 2, smile_count = 0 WHERE photo_id = ?1",
        [a1],
    )
    .unwrap();
    // b2 has no capture datetime → the engine falls back to indexed_at.
    // Pin it so the fallback is deterministic instead of "seed time now".
    conn.execute(
        "UPDATE photos SET indexed_at = ?1 WHERE filename = 'b2.jpg'",
        ["2026-07-05T09:00:00Z"],
    )
    .unwrap();
    drop(conn);

    // Selection signal: selection state + file operations.
    let conn = db.lock().unwrap();
    conn.execute(
        "INSERT INTO selections (photo_id, state, updated_at) VALUES (?1, 'selected', ?2), (?3, 'rejected', ?2)",
        rusqlite::params![a1, "2026-08-17T00:00:00Z", a2],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO file_operations (op_type, source_path, dest_path, status, detail, created_at)
         VALUES ('move', '/shoot/a/k1.jpg', '/culls/k1.jpg', 'done', NULL, ?1),
                ('trash', '/old/x.jpg', NULL, 'done', NULL, ?1)",
        ["2026-08-16T00:00:00Z"],
    )
    .unwrap();
    drop(conn);

    (a, b)
}

fn db(label: &str) -> Db {
    let path = std::env::temp_dir().join(format!(
        "pg_stats_it_{label}_{}.sqlite",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let db = Db::open(&path).unwrap();
    db.migrate().unwrap();
    db
}

#[test]
fn all_time_totals_averages_and_shares() {
    let db = db("all");
    seed(&db);
    let s = period_stats(&db, &parse_period("").unwrap(), now()).unwrap();

    assert_eq!(s.period, "All time");
    assert_eq!(s.photos, 4);
    assert_eq!(s.sessions, 2);
    assert!((s.photos_per_session.unwrap() - 2.0).abs() < 1e-9);

    // Analyzed-only averages; the denominator is reported.
    assert_eq!(s.analyzed, 3);
    assert!((s.avg_sharpness.unwrap() - 185.0 / 3.0).abs() < 1e-9);
    assert!((s.avg_brightness.unwrap() - 60.0).abs() < 1e-9);
    assert!((s.avg_contrast.unwrap() - 50.0).abs() < 1e-9);
    assert!((s.avg_saturation.unwrap() - 122.0 / 3.0).abs() < 1e-9);

    // Shares over analyzed: 1 of 3 monochrome.
    assert!((s.monochrome_share.unwrap() - 100.0 / 3.0).abs() < 1e-9);
    assert!((s.color_share.unwrap() - 200.0 / 3.0).abs() < 1e-9);

    // AI shares only over photos with data: a1 (faces yes, smile no).
    assert!((s.faces_present_share.unwrap() - 100.0).abs() < 1e-9);
    assert!((s.smiling_share.unwrap() - 0.0).abs() < 1e-9);
}

#[test]
fn histograms_use_the_fixed_bins() {
    let db = db("hist");
    seed(&db);
    let s = period_stats(&db, &parse_period("").unwrap(), now()).unwrap();

    let sums: Vec<u32> = s.iso_histogram.iter().map(|b| b.count).collect();
    assert_eq!(sums, vec![1, 1, 0, 1, 0]); // 100, 400, –, 1600, –
    assert_eq!(s.iso_histogram[1].label, "400–800");

    let sums: Vec<u32> = s.aperture_histogram.iter().map(|b| b.count).collect();
    assert_eq!(sums, vec![0, 0, 2, 1, 0]); // 2.8×2, 4.0

    let sums: Vec<u32> = s.focal_histogram.iter().map(|b| b.count).collect();
    assert_eq!(sums, vec![0, 0, 2, 0, 0]); // 50mm ×2

    let sums: Vec<u32> = s.shutter_histogram.iter().map(|b| b.count).collect();
    assert_eq!(sums, vec![0, 1, 0, 0, 2, 0, 0]); // 1/2 ×1, 1/250 ×2
}

#[test]
fn usage_tables_report_counts_shares_and_analyzed_only_averages() {
    let db = db("usage");
    seed(&db);
    let s = period_stats(&db, &parse_period("").unwrap(), now()).unwrap();

    // Three makes: Sony ×2, Canon ×1, and b2 (no EXIF) groups as unknown.
    assert_eq!(s.camera_usage.len(), 3);
    let sony = &s.camera_usage[0];
    assert_eq!(sony.name, "Sony");
    assert_eq!(sony.photos, 2);
    assert!((sony.share - 50.0).abs() < 1e-9);
    assert!((sony.avg_sharpness.unwrap() - 62.5).abs() < 1e-9);
    assert!((sony.avg_iso.unwrap() - 250.0).abs() < 1e-9);
    let canon = &s.camera_usage[1];
    assert_eq!(canon.name, "Canon");
    assert_eq!(canon.photos, 1);
    assert!(canon.avg_sharpness.is_none()); // its only photo is unanalyzed
    assert!((canon.avg_iso.unwrap() - 1600.0).abs() < 1e-9);
    let unknown = &s.camera_usage[2];
    assert_eq!(unknown.name, "Unknown camera");
    assert_eq!(unknown.photos, 1);
    assert!((unknown.avg_sharpness.unwrap() - 60.0).abs() < 1e-9);
    assert!(unknown.avg_iso.is_none());

    let known = &s.lens_usage[0];
    assert_eq!(known.name, "50mm F1.4");
    let unknown = s.lens_usage.iter().find(|l| l.name == "Unknown lens").unwrap();
    assert_eq!(unknown.photos, 2);
    assert!((unknown.avg_sharpness.unwrap() - 60.0).abs() < 1e-9); // b2 only
    assert!((unknown.avg_iso.unwrap() - 1600.0).abs() < 1e-9); // b1 only
}

#[test]
fn trend_contains_only_months_with_data() {
    let db = db("trend");
    seed(&db);
    let s = period_stats(&db, &Period::All, now()).unwrap();

    assert_eq!(s.trend.len(), 2);
    assert_eq!(s.trend[0].month, "2026-06");
    assert_eq!(s.trend[1].month, "2026-07");
    assert_eq!(s.trend[0].photos, 2);
    assert_eq!(s.trend[0].sessions, 1);
    assert!((s.trend[0].avg_sharpness.unwrap() - 62.5).abs() < 1e-9);
    assert!((s.trend[0].avg_iso.unwrap() - 250.0).abs() < 1e-9);
    assert!((s.trend[0].color_share.unwrap() - 50.0).abs() < 1e-9);
    assert!((s.trend[1].avg_sharpness.unwrap() - 60.0).abs() < 1e-9); // b2 only
    assert!((s.trend[1].color_share.unwrap() - 100.0).abs() < 1e-9);
}

#[test]
fn custom_period_scopes_everything_including_selection() {
    let db = db("custom");
    seed(&db);
    let s = period_stats(
        &db,
        &Period::Custom {
            from: "2026-06-01".into(),
            to: "2026-06-30".into(),
        },
        now(),
    )
    .unwrap();

    assert_eq!(s.photos, 2);
    assert_eq!(s.sessions, 1);
    assert_eq!(s.analyzed, 2);
    assert!((s.avg_sharpness.unwrap() - 62.5).abs() < 1e-9);
    let sum: u32 = s.iso_histogram.iter().map(|b| b.count).sum();
    assert_eq!(sum, 2); // June's two ISOs only

    // Selection is scoped by period; the trash count is global.
    let sel = s.selection.as_ref().unwrap();
    assert_eq!(sel.imported, 2);
    assert_eq!(sel.selected, 1);
    assert_eq!(sel.rejected, 1);
    assert_eq!(sel.trashed, 1);
    assert!((sel.kept_ratio.unwrap() - 0.5).abs() < 1e-9);
}

#[test]
fn empty_period_is_honest_zero_not_unavailable_confusion() {
    let db = db("emptyperiod");
    seed(&db);
    let s = period_stats(&db, &Period::ThisMonth, now()).unwrap(); // August

    assert_eq!(s.photos, 0);
    assert_eq!(s.sessions, 0);
    assert!(s.photos_per_session.is_none());
    assert_eq!(s.analyzed, 0);
    for avg in [
        s.avg_sharpness,
        s.avg_brightness,
        s.avg_contrast,
        s.avg_saturation,
    ] {
        assert!(avg.is_none(), "unanalyzed period must report None");
    }
    assert!(s.monochrome_share.is_none());
    assert!(s.color_share.is_none());
    assert!(s.faces_present_share.is_none());
    assert!(s.smiling_share.is_none());
    assert!(s.trend.is_empty());
    assert!(s.camera_usage.is_empty());
    // A selection signal exists globally (the trash op), so the section is
    // present but with zero in-scope counts and no ratio.
    let sel = s.selection.as_ref().unwrap();
    assert_eq!(sel.imported, 0);
    assert!(sel.kept_ratio.is_none());
    assert_eq!(sel.trashed, 1);
}

#[test]
fn selection_is_hidden_when_no_signal_exists() {
    let db = db("noselect");
    seed(&db);
    // Strip every selection signal → the ratio section must disappear.
    let conn = db.lock().unwrap();
    conn.execute("DELETE FROM selections", []).unwrap();
    conn.execute("DELETE FROM file_operations", []).unwrap();
    drop(conn);

    let s = period_stats(&db, &Period::All, now()).unwrap();
    assert!(s.selection.is_none(), "no signal → section hidden");
}

#[test]
fn session_summary_scopes_stats_and_reports_duration() {
    let db = db("summary");
    let (a, _b) = seed(&db);
    // The app refreshes denormalized session fields after passes.
    db.refresh_all_sessions_times().unwrap();
    let sum = session_summary(&db, a).unwrap();

    assert_eq!(sum.session.name, "June shoot");
    assert_eq!(sum.session.photo_count, 2);
    // Jun 15 10:00 → Jun 20 15:00 = 5 days + 5 hours.
    assert!((sum.duration_days.unwrap() - (5.0 + 5.0 / 24.0)).abs() < 1e-9);

    assert_eq!(sum.stats.period, "Session: June shoot");
    assert_eq!(sum.stats.photos, 2);
    assert_eq!(sum.stats.analyzed, 2);
    assert!((sum.stats.avg_sharpness.unwrap() - 62.5).abs() < 1e-9);
    assert!((sum.stats.monochrome_share.unwrap() - 50.0).abs() < 1e-9);
}

#[test]
fn unknown_session_fails_friendly() {
    let db = db("unknown");
    seed(&db);
    assert!(session_summary(&db, 9999).is_err());
}

#[test]
fn compare_sessions_puts_same_metric_rows_side_by_side() {
    let db = db("compare");
    let (a, b) = seed(&db);
    let rows = compare_sessions(&db, vec![a, b]).unwrap();
    assert_eq!(rows.len(), 2);

    let ja = &rows[0];
    assert_eq!(ja.name, "June shoot");
    assert_eq!(ja.photos, 2);
    assert_eq!(ja.analyzed, 2);
    assert!((ja.avg_sharpness.unwrap() - 62.5).abs() < 1e-9);
    assert!((ja.avg_iso.unwrap() - 250.0).abs() < 1e-9);
    assert!((ja.monochrome_share.unwrap() - 50.0).abs() < 1e-9);

    let jb = &rows[1];
    assert_eq!(jb.name, "July shoot");
    assert_eq!(jb.photos, 2);
    assert_eq!(jb.analyzed, 1);
    assert!((jb.avg_sharpness.unwrap() - 60.0).abs() < 1e-9);
    assert!((jb.avg_iso.unwrap() - 1600.0).abs() < 1e-9);
    // July has one analyzed photo and it is color.
    assert!((jb.monochrome_share.unwrap() - 0.0).abs() < 1e-9);
    assert!((jb.color_share.unwrap() - 100.0).abs() < 1e-9);
    // 2026-07-01T12:00 → 2026-07-05T09:00 (b2 falls back to indexed_at).
    assert!((jb.duration_days.unwrap() - 3.875).abs() < 1e-9);
}

#[test]
fn compare_rejects_bad_sizes_and_unknown_ids() {
    let db = db("comparebad");
    let (a, _b) = seed(&db);
    assert!(compare_sessions(&db, Vec::new()).is_err());
    let nine = vec![a; 9];
    assert!(compare_sessions(&db, nine).is_err());
    assert!(compare_sessions(&db, vec![a, 424242]).is_err());
}

#[test]
fn session_times_refresh_derives_shoot_period_from_photos() {
    let db = db("times");
    let (a, b) = seed(&db);
    // Before the refresh, sessions carry no times.
    let pre = db.session_by_id(a).unwrap().unwrap();
    assert!(pre.start_time.is_none());
    assert!(pre.end_time.is_none());

    db.refresh_all_sessions_times().unwrap();

    let a = db.session_by_id(a).unwrap().unwrap();
    assert_eq!(a.start_time.as_deref(), Some("2026-06-15T10:00:00Z"));
    assert_eq!(a.end_time.as_deref(), Some("2026-06-20T15:00:00Z"));
    let b = db.session_by_id(b).unwrap().unwrap();
    // b2 has no capture datetime → its pinned indexed_at is the max.
    assert_eq!(b.start_time.as_deref(), Some("2026-07-01T12:00:00Z"));
    assert_eq!(b.end_time.as_deref(), Some("2026-07-05T09:00:00Z"));
}
