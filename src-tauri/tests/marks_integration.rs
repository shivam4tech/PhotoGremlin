//! Integration test: curatorial marks (Sprint 13) through the real query
//! path — bulk set/clear via `set_marks`, marks on grid rows, and
//! rating/flag/color filters through `photos_where` with the real filter
//! builder.

use std::path::PathBuf;

use photogremlin_lib::database::{Db, PhotoUpsert};
use photogremlin_lib::filters;

struct Env {
    root: PathBuf,
    db: Db,
}

impl Env {
    fn new() -> Env {
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("pg_marks_{n}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let db = Db::open(&root.join("database.sqlite")).unwrap();
        db.migrate().unwrap();
        Env { root, db }
    }

    fn add(&self, name: &str) -> i64 {
        self.db
            .upsert_photo(&PhotoUpsert {
                path: self.root.join(name).display().to_string(),
                filename: name.to_string(),
                extension: name.rsplit('.').next().unwrap_or("jpg").to_string(),
                size_bytes: Some(1),
                width: Some(10),
                height: Some(10),
                orientation: None,
                session_id: None,
                file_mtime: None,
            })
            .unwrap()
    }

    fn filtered(&self, filter_json: &str) -> Vec<String> {
        let filter = filters::parse_filter(filter_json).unwrap();
        let (sql, params) = filters::build_where(&filter).unwrap();
        let (photos, _total) = self.db.photos_where(&sql, params, 0, 100).unwrap();
        photos.iter().map(|p| p.filename.clone()).collect()
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn mark_filter(field: &str, op: &str, value: serde_json::Value) -> String {
    serde_json::json!({
        "operator": "AND",
        "conditions": [{ "field": field, "operator": op, "value": value }]
    })
    .to_string()
}

#[test]
fn marks_flow_through_grid_rows_and_filters() {
    let e = Env::new();
    let a = e.add("rated.jpg");
    let b = e.add("flagged.jpg");
    let c = e.add("labeled.jpg");
    let d = e.add("plain.jpg");

    // Seed: a rated+flagged, b flagged only, c color-labeled, d clean.
    e.db.set_marks(&[a, b], Some(5), Some(true), None).unwrap();
    e.db.set_marks(&[c], None, None, Some("blue")).unwrap();

    // Grid rows carry the marks.
    let (all, _) = e.db.photos_where("", vec![], 0, 100).unwrap();
    let by_name = |name: &str| all.iter().find(|p| p.filename == name).unwrap();
    assert_eq!(by_name("rated.jpg").rating, Some(5));
    assert!(by_name("rated.jpg").flag);
    assert_eq!(by_name("flagged.jpg").rating, Some(5));
    assert!(by_name("flagged.jpg").flag);
    assert_eq!(by_name("labeled.jpg").color_label.as_deref(), Some("blue"));
    assert_eq!(by_name("plain.jpg").rating, None);
    assert!(!by_name("plain.jpg").flag);
    assert_eq!(by_name("plain.jpg").color_label, None);

    // Filters: rated >= 4 → both 5-star photos.
    assert_eq!(
        e.filtered(&mark_filter("rating", ">=", serde_json::json!(4))),
        vec!["rated.jpg", "flagged.jpg"]
    );
    // Unrated (rating is null) → the two unrated photos.
    assert_eq!(
        e.filtered(&mark_filter("rating", "is-null", serde_json::json!(null))),
        vec!["labeled.jpg", "plain.jpg"]
    );
    // Flagged → a and b.
    assert_eq!(
        e.filtered(&mark_filter("flagged", "=", serde_json::json!(true))),
        vec!["rated.jpg", "flagged.jpg"]
    );
    // Color label red → nobody; blue → c.
    assert!(e.filtered(&mark_filter("color_label", "=", serde_json::json!("red"))).is_empty());
    assert_eq!(
        e.filtered(&mark_filter("color_label", "=", serde_json::json!("blue"))),
        vec!["labeled.jpg"]
    );

    // Clear batch: unrate everyone, unflag a+c, label d red.
    e.db.set_marks(&[a, b, c, d], Some(0), Some(false), Some("red")).unwrap();
    let (all, _) = e.db.photos_where("", vec![], 0, 100).unwrap();
    let by_name = |name: &str| all.iter().find(|p| p.filename == name).unwrap();
    assert_eq!(by_name("rated.jpg").rating, None);
    assert!(!by_name("flagged.jpg").flag);
    assert_eq!(by_name("plain.jpg").color_label.as_deref(), Some("red"));
}
