//! Sprint 18 (scene classification) integration tests: the scene pass
//! end-to-end against the embedded stub model, the incremental stamp /
//! re-run rules, and the `scene_group` filter field.
//!
//! Like the face-pass tests, these require ONNX Runtime on the test machine
//! (it is on the dev box); where unavailable they skip — the degradation
//! contract is that queue/storage invariants still hold without it.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use image::{ImageBuffer, Rgb};
use photogremlin_lib::database::{Db, PhotoUpsert};
use photogremlin_lib::events::ProgressPayload;
use photogremlin_lib::ml::scene;
use photogremlin_lib::ml;

const MTIME: &str = "2026-01-01T00:00:00Z";
const MTIME_NEW: &str = "2026-06-01T00:00:00Z";

const MERGED: &[&str] = &[
    "nature",
    "urban",
    "home_stay",
    "public_indoor",
    "faith_history",
    "sports_leisure",
    "food_night",
    "transport",
    "industry",
    "other",
];

struct Env {
    root: PathBuf,
    db: Arc<Db>,
}

impl Env {
    fn new() -> Env {
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir()
            .join(format!("pg_sprint18_it_{n}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let db_path = root.join("db.sqlite");
        let db = Arc::new(Db::open(&db_path).unwrap());
        db.migrate().unwrap();
        Env { root, db }
    }

    fn add_photo(&self, name: &str, bytes_len_hint: u8, file_mtime: &str) -> i64 {
        let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(320, 240, Rgb([bytes_len_hint, 90, 60]));
        let path = self.root.join(name);
        img.save_with_format(&path, image::ImageFormat::Jpeg).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let (w, h) = image::image_dimensions(&path).unwrap();
        self.db
            .upsert_photo(&PhotoUpsert {
                path: path.to_string_lossy().into_owned(),
                filename: name.to_string(),
                extension: "jpg".to_string(),
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

fn run_pass(db: &Arc<Db>) -> scene::SceneSummary {
    scene::run_scenes_pass(
        db.clone(),
        Arc::new(|_p: ProgressPayload| {}),
        Arc::new(AtomicBool::new(false)),
    )
    .expect("scene pass returned Err (see friendly message in log)")
}

fn stored_scenes(db: &Arc<Db>) -> Vec<(i64, Option<String>, Option<String>, Option<f64>)> {
    let conn = db.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT photo_id, scene_coarse, scene_fine, scene_conf
                  FROM analysis WHERE scene_fine IS NOT NULL ORDER BY photo_id")
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .unwrap();
    rows.map(|r| r.unwrap()).collect()
}

#[test]
fn scenes_pass_labels_all_photos_and_queue_drains() {
    let env = Env::new();
    let a = env.add_photo("a.jpg", 10, MTIME);
    let b = env.add_photo("b.jpg", 200, MTIME);

    let summary = run_pass(&env.db);
    assert_eq!(summary.processed, 2, "{summary:?}");
    assert!(!summary.cancelled);

    let scenes = stored_scenes(&env.db);
    assert_eq!(scenes.len(), 2);
    for (pid, coarse, fine, conf) in &scenes {
        assert!(
            MERGED.contains(&coarse.as_deref().unwrap_or("")),
            "coarse {coarse:?} not a merged product group"
        );
        assert!(fine.as_deref().unwrap_or("").len() > 0);
        let c = conf.unwrap();
        assert!((0.0..=1.0).contains(&c), "conf {c} out of range");
        assert!(pid == &a || pid == &b);
    }

    // queue drains after the pass
    assert!(env.db.scenes_queue().unwrap().is_empty());
}

#[test]
fn scenes_queue_is_incremental_on_mtime() {
    let env = Env::new();
    let id = env.add_photo("a.jpg", 42, MTIME);
    run_pass(&env.db);
    assert!(env.db.scenes_queue().unwrap().is_empty());

    // simulate an external edit: newer mtime recorded for the same file
    let path = env.root.join("a.jpg");
    let conn = env.db.lock().unwrap();
    conn.execute(
        "UPDATE photos SET file_mtime = ?1 WHERE id = ?2",
        rusqlite::params![MTIME_NEW, id],
    )
    .unwrap();
    drop(conn);

    let queue = env.db.scenes_queue().unwrap();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].photo_id, id);
}

#[test]
fn scene_group_filter_matches_stored_group() {
    let env = Env::new();
    let id = env.add_photo("a.jpg", 7, MTIME);
    run_pass(&env.db);

    let scenes = stored_scenes(&env.db);
    let (_pid, coarse, _fine, _conf) = &scenes
        .iter()
        .find(|(pid, ..)| *pid == id)
        .expect("photo classified")
        ;
    let group = coarse.clone().unwrap();

    let json = format!(
        r#"{{"operator":"AND","conditions":[{{"field":"scene_group","operator":"=","value":{}}}]}}"#,
        serde_json::to_string(&group).unwrap()
    );
    let filter = photogremlin_lib::filters::parse_filter(&json).unwrap();
    let (where_sql, params) = photogremlin_lib::filters::build_where(&filter).unwrap();

    let conn = env.db.lock().unwrap();
    let mut stmt = conn
        .prepare(&format!(
            "SELECT p.id FROM photos p LEFT JOIN analysis a ON a.photo_id = p.id {where_sql}"
        ))
        .unwrap();
    let ids: Vec<i64> = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), |r| r.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert!(ids.contains(&id), "filter '{group}' should match photo");
}

/// The pass reports per-file failures as friendly errors and keeps going.
#[test]
fn scenes_pass_survives_a_missing_file() {
    let env = Env::new();
    env.add_photo("gone.jpg", 1, MTIME);
    // remove the actual file; the DB row still points at it
    std::fs::remove_file(env.root.join("gone.jpg")).unwrap();
    env.add_photo("kept.jpg", 3, MTIME);

    let summary = run_pass(&env.db);
    assert_eq!(summary.failed, 1);
    assert_eq!(summary.processed, 1);
    assert!(!summary.errors.is_empty(), "friendly error expected");
}

// keep imports used even if tests are skipped on machines without ORT
#[allow(dead_code)]
fn touch(_: &Mutex<()>) {}
const _: fn() = || {
    let _ = ml::runtime_status;
};
