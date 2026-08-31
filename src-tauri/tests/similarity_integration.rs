//! Integration test: saved views, collections, and the similarity pass
//! (Sprint 8) against real files + a real temp DB.
//!
//! Covers: the filter engine's `session_id` field (the "open session in
//! library" path), saved-view CRUD + dynamic counts, collection CRUD +
//! membership + cascades, and the full similarity pipeline on real JPEGs
//! (dHash → similar groups + bursts, incremental re-hash on mtime change,
//! group persistence with covers, cancellation).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use photogremlin_lib::{
    database::{Db, FaceObservationInput, GroupPhotoSort, PhotoUpsert},
    events::ProgressPayload,
    filters,
    similarity,
};

/// Deterministic test JPEG. `ramp` = left→right gradient, `stripes` =
/// vertical stripes (clearly different structure from a ramp), `flat` =
/// constant gray (far from both).
fn jpeg_bytes(w: u32, h: u32, pattern: &str, quality: u8) -> Vec<u8> {
    let img = image::RgbImage::from_fn(w, h, |x, y| {
        let v: u8 = match pattern {
            "ramp" => (x * 255 / w.saturating_sub(1)).min(255) as u8,
            "stripes" => {
                if (x / 8) % 2 == 0 {
                    (60 + y % 40) as u8
                } else {
                    (200 - y % 40) as u8
                }
            }
            _ => 128,
        };
        image::Rgb([v, v, v])
    });
    let mut out: Vec<u8> = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality)
        .encode(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    out
}

struct Env {
    root: PathBuf,
    db: Arc<Db>,
}

impl Env {
    /// `files` are (name, scene pattern); everything lands in one session.
    fn new(files: &[(&str, &str)]) -> Env {
        static COUNTER: std::sync::atomic::AtomicUsize =
            std::sync::atomic::AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir()
            .join(format!("pg_sprint8_it_{n}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let shoot = root.join("shoot");
        std::fs::create_dir_all(&shoot).unwrap();
        for (name, pattern) in files {
            // One quality per name → identical names would collide anyway;
            // q95 keeps the re-encode pair (a, a2) byte-stable.
            let q = if name.starts_with('a') { 95 } else { 80 };
            std::fs::write(shoot.join(*name), jpeg_bytes(64, 48, pattern, q)).unwrap();
        }
        let db_path = root.join("db.sqlite");
        let _ = std::fs::remove_file(&db_path);
        let db = Arc::new(Db::open(&db_path).unwrap());
        db.migrate().unwrap();
        let session = db.upsert_session("Shoot", Some(shoot.to_str().unwrap())).unwrap();
        db.set_setting("active_folder", shoot.to_str().unwrap()).unwrap();
        for (name, _) in files {
            let p = shoot.join(name);
            db.upsert_photo(&PhotoUpsert {
                path: p.to_string_lossy().into_owned(),
                filename: name.to_string(),
                extension: "jpg".into(),
                size_bytes: Some(123),
                width: Some(64),
                height: Some(48),
                orientation: if name.starts_with('a') {
                    Some("landscape".into())
                } else {
                    Some("portrait".into())
                },
                session_id: Some(session),
                file_mtime: Some("2026-01-01T00:00:00Z".into()),
            })
            .unwrap();
        }
        Env { root, db }
    }

    fn ids_for(&self, names: &[&str]) -> Vec<i64> {
        let conn = self.db.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, filename FROM photos ORDER BY filename")
            .unwrap();
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .unwrap();
        rows.collect::<Result<Vec<_>, _>>()
            .unwrap()
            .into_iter()
            .filter(|(_, f)| names.contains(&f.as_str()))
            .map(|(id, _)| id)
            .collect()
    }

    fn raw(&self, sql: &str) {
        self.db.lock().unwrap().execute(sql, []).unwrap();
    }

    fn one_i64(&self, sql: &str) -> i64 {
        self.db
            .lock()
            .unwrap()
            .query_row(sql, [], |r| r.get::<_, i64>(0))
            .unwrap()
    }

    fn run_similarity(&self) -> similarity::SimilaritySummary {
        let progress: Arc<dyn Fn(ProgressPayload) + Send + Sync> =
            Arc::new(|_: ProgressPayload| {});
        let cancel = Arc::new(AtomicBool::new(false));
        similarity::run_similarity(self.db.clone(), progress, cancel).unwrap()
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn group_contains(db: &Db, group_id: i64, photo_id: i64) -> bool {
    db.lock()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM similarity_group_photos WHERE group_id = ?1 AND photo_id = ?2",
            [group_id, photo_id],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
        > 0
}

// ---------------------------------------------------------------------------
// Filter engine: session_id field (the "open session in library" path)
// ---------------------------------------------------------------------------

#[test]
fn session_id_filter_field_scopes_photos_where() {
    let env = Env::new(&[("a.jpg", "ramp"), ("b.jpg", "stripes")]);
    let s: i64 = env.one_i64("SELECT id FROM sessions ORDER BY id LIMIT 1");
    let s2 = env.db.upsert_session("Other", None).unwrap();
    let id = env.ids_for(&["b.jpg"])[0];
    env.raw(&format!("UPDATE photos SET session_id = {s2} WHERE id = {id}"));

    let json = format!(
        r#"{{"operator":"AND","conditions":[{{"field":"session_id","operator":"=","value":{s}}}]}}"#
    );
    let filter = filters::parse_filter(&json).unwrap();
    let (where_sql, params) = filters::build_where(&filter).unwrap();
    let (photos, total) = env.db.photos_where(&where_sql, params, 0, 50).unwrap();
    assert_eq!(total, 1);
    assert_eq!(photos.len(), 1);
    assert_eq!(photos[0].id, env.ids_for(&["a.jpg"])[0]);
}

// ---------------------------------------------------------------------------
// Saved views
// ---------------------------------------------------------------------------

#[test]
fn saved_view_crud_with_dynamic_count() {
    let env = Env::new(&[
        ("a.jpg", "ramp"),
        ("a2.jpg", "ramp"),
        ("b.jpg", "stripes"),
        ("c.jpg", "ramp"),
    ]);
    let f_json = r#"{"operator":"AND","conditions":[{"field":"orientation","operator":"=","value":"landscape"}]}"#;
    // a.jpg + a2.jpg are landscape.
    let id = env.db.upsert_saved_view("Landscapes", f_json, None).unwrap();
    // Same name again → overwrite (new description), same id.
    let id2 = env
        .db
        .upsert_saved_view("Landscapes", f_json, Some("wide shots"))
        .unwrap();
    assert_eq!(id, id2);
    let views = env.db.list_saved_views().unwrap();
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].name, "Landscapes");
    assert_eq!(views[0].description.as_deref(), Some("wide shots"));

    // The count is dynamic: it follows the current library.
    let filter = filters::parse_filter(f_json).unwrap();
    let (where_sql, params) = filters::build_where(&filter).unwrap();
    let (_, total) = env.db.photos_where(&where_sql, params, 0, 1).unwrap();
    assert_eq!(total, 2);

    // Rename + delete (the other view survives).
    env.db.rename_saved_view(id, "Landscape picks").unwrap();
    assert_eq!(
        env.db.list_saved_views().unwrap()[0].name,
        "Landscape picks"
    );
    let empty = r#"{"operator":"AND","conditions":[]}"#;
    env.db.upsert_saved_view("All", empty, None).unwrap();
    env.db.delete_saved_view(id).unwrap();
    let remaining = env.db.list_saved_views().unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].name, "All");

    // Blank names are rejected with a validation error.
    assert!(matches!(
        env.db.upsert_saved_view("   ", empty, None),
        Err(photogremlin_lib::error::AppError::Validation { .. })
    ));
}

// ---------------------------------------------------------------------------
// Collections
// ---------------------------------------------------------------------------

#[test]
fn collection_crud_membership_and_cascade() {
    let env = Env::new(&[("a.jpg", "ramp"), ("b.jpg", "stripes"), ("c.jpg", "ramp")]);
    let ids = env.ids_for(&["a.jpg", "b.jpg", "c.jpg"]);

    let col = env.db.create_collection("Wedding selects", None).unwrap();
    // Duplicate name → friendly validation error.
    assert!(env.db.create_collection("Wedding selects", None).is_err());

    assert_eq!(env.db.add_to_collection(col, ids.clone()).unwrap(), 3);
    // Idempotent — re-adding changes nothing.
    assert_eq!(env.db.add_to_collection(col, ids.clone()).unwrap(), 0);

    let cols = env.db.list_collections().unwrap();
    assert_eq!(cols.len(), 1);
    assert_eq!(cols[0].name, "Wedding selects");
    assert_eq!(cols[0].photo_count, 3);

    // Pagination over the membership.
    let (page, total) = env.db.collection_photos(col, 0, 2).unwrap();
    assert_eq!(total, 3);
    assert_eq!(page.len(), 2);
    let (rest, _) = env.db.collection_photos(col, 2, 2).unwrap();
    assert_eq!(rest.len(), 1);

    // Remove one; the photo itself must survive.
    assert_eq!(env.db.remove_from_collection(col, vec![ids[0]]).unwrap(), 1);
    assert_eq!(env.db.list_collections().unwrap()[0].photo_count, 2);
    assert_eq!(env.one_i64("SELECT COUNT(*) FROM photos"), 3);

    // Rename + duplicate rename rejected.
    let other = env.db.create_collection("Other", None).unwrap();
    env.db.rename_collection(col, "Best of the day").unwrap();
    assert!(env.db.rename_collection(other, "Best of the day").is_err());

    // Delete cascades membership but never touches photos.
    env.db.delete_collection(col).unwrap();
    assert_eq!(
        env.db
            .list_collections()
            .unwrap()
            .iter()
            .all(|c| c.id != col),
        true
    );
    assert_eq!(env.one_i64("SELECT COUNT(*) FROM collection_photos"), 0);
    assert_eq!(env.one_i64("SELECT COUNT(*) FROM photos"), 3);

    // A missing collection is a friendly error, not a panic.
    assert!(env.db.add_to_collection(9999, vec![ids[0]]).is_err());
    assert!(env.db.collection_photos(9999, 0, 10).is_err());
}

// ---------------------------------------------------------------------------
// Similarity pass on real JPEGs
// ---------------------------------------------------------------------------

#[test]
fn similarity_hashing_groups_similar_and_bursts() {
    // a + a2: the same scene, re-encoded (near-duplicate → similar group).
    // b: clearly different structure. d: flat gray, a third distinct scene.
    let env = Env::new(&[
        ("a.jpg", "ramp"),
        ("a2.jpg", "ramp"),
        ("b.jpg", "stripes"),
        ("d.jpg", "flat"),
    ]);
    // Capture times: a, a2, b are one burst (≤3s apart); d is 30s later.
    env.raw(
        "UPDATE photos SET capture_datetime = '2026-08-16T10:00:00Z' \
         WHERE filename IN ('a.jpg','a2.jpg','b.jpg')",
    );
    env.raw(
        "UPDATE photos SET capture_datetime = '2026-08-16T10:00:30Z' \
         WHERE filename = 'd.jpg'",
    );

    let s = env.run_similarity();
    assert_eq!(s.hashed, 4, "all four decodable files must be hashed");
    assert_eq!(s.failed, 0);
    assert!(!s.cancelled);
    assert!(
        s.similar_groups >= 1,
        "the re-encoded pair must form a similar group, got {s:?}"
    );
    assert_eq!(
        s.burst_groups, 1,
        "only the ≤3s trio bursts (d is 30s later), got {s:?}"
    );

    // Incremental: an up-to-date library re-hashes nothing.
    let s2 = env.run_similarity();
    assert_eq!(s2.hashed, 0, "up-to-date library must re-hash nothing");
    assert_eq!(s2.similar_groups, s.similar_groups);

    // A changed mtime re-queues exactly that one photo.
    env.raw("UPDATE photos SET file_mtime = '2026-08-16T12:00:00Z' WHERE filename = 'b.jpg'");
    let s3 = env.run_similarity();
    assert_eq!(s3.hashed, 1, "only the modified file is re-hashed");

    // Persisted groups: the similar group holds exactly a + a2; the burst
    // group holds a + a2 + b and never d.
    let groups = env.db.list_similarity_groups(50).unwrap();
    let a = env.ids_for(&["a.jpg"])[0];
    let a2 = env.ids_for(&["a2.jpg"])[0];
    let b = env.ids_for(&["b.jpg"])[0];
    let d = env.ids_for(&["d.jpg"])[0];

    let similar = groups
        .iter()
        .find(|g| {
            g.group_type == "similar"
                && g.cover_photos.contains(&a)
                && g.cover_photos.contains(&a2)
        })
        .expect("a + a2 must share a similar group");
    assert_eq!(similar.photo_count, 2, "only the pair is similar: {groups:?}");
    assert!(!group_contains(&env.db, similar.id, d), "d is a distinct scene");
    assert!(!group_contains(&env.db, similar.id, b), "b is a distinct scene");

    let burst = groups
        .iter()
        .find(|g| {
            g.group_type == "burst"
                && g.cover_photos.contains(&a)
                && g.cover_photos.contains(&a2)
                && g.cover_photos.contains(&b)
        })
        .expect("a, a2, b (≤3s apart) must burst");
    assert!(!group_contains(&env.db, burst.id, d), "d is 30s later");
    assert_eq!(burst.photo_count, 3);
}

#[test]
fn groups_are_saved_per_project_and_never_cross_into_the_active_project() {
    // Each project gets a durable independent result set. The second project
    // deliberately has a re-encoded scene from the first: opening it must
    // never display or merge the first project's groups.
    let env = Env::new(&[
        ("b.jpg", "stripes"),
        ("c.jpg", "stripes"),
        ("d.jpg", "flat"),
    ]);
    let shoot2 = env.root.join("shoot2");
    std::fs::create_dir_all(&shoot2).unwrap();
    std::fs::write(shoot2.join("b2.jpg"), jpeg_bytes(64, 48, "stripes", 70)).unwrap();
    std::fs::write(shoot2.join("d2.jpg"), jpeg_bytes(64, 48, "flat", 70)).unwrap();
    let s2 = env
        .db
        .upsert_session("Shoot 2", Some(shoot2.to_str().unwrap()))
        .unwrap();
    for name in ["b2.jpg", "d2.jpg"] {
        let p = shoot2.join(name);
        env.db
            .upsert_photo(&PhotoUpsert {
                path: p.to_string_lossy().into_owned(),
                filename: name.to_string(),
                extension: "jpg".into(),
                size_bytes: Some(123),
                width: Some(64),
                height: Some(48),
                orientation: Some("landscape".into()),
                session_id: Some(s2),
                file_mtime: Some("2026-01-01T00:00:00Z".into()),
            })
            .unwrap();
    }
    // Same absolute seconds in both sessions: bursts and all similar groups
    // must still stay project-scoped.
    env.raw(
        "UPDATE photos SET capture_datetime = '2026-08-16T10:00:00Z' \
         WHERE filename IN ('b.jpg','c.jpg')",
    );
    env.raw(
        "UPDATE photos SET capture_datetime = '2026-08-16T10:00:02Z' \
         WHERE filename = 'b2.jpg'",
    );
    env.raw(
        "UPDATE photos SET capture_datetime = '2026-08-16T10:00:30Z' \
         WHERE filename IN ('d.jpg','d2.jpg')",
    );

    let first = env.run_similarity();
    assert_eq!(first.hashed, 3, "{first:?}");

    let groups = env.db.list_similarity_groups(50).unwrap();
    let b = env.ids_for(&["b.jpg"])[0];
    let c = env.ids_for(&["c.jpg"])[0];
    let d = env.ids_for(&["d.jpg"])[0];
    let b2 = env.ids_for(&["b2.jpg"])[0];
    let d2 = env.ids_for(&["d2.jpg"])[0];

    // This project's within-session group exists and never includes the
    // other project's re-encoded file.
    let within = groups
        .iter()
        .find(|g| g.group_type == "similar" && g.photo_count == 2 && g.session_count == 1)
        .expect("b + c must keep their within-session group");
    assert!(group_contains(&env.db, within.id, b));
    assert!(group_contains(&env.db, within.id, c));

    // Bursts: only session 1's b + c.
    assert_eq!(first.burst_groups, 1, "bursts never span sessions: {first:?}");
    for g in groups.iter().filter(|g| g.group_type == "burst") {
        assert!(!group_contains(&env.db, g.id, b2), "burst must not include b2");
    }

    // The active project switches to shoot 2. Its run does not delete shoot
    // 1's groups and it cannot show those old cards.
    env.db.set_setting("active_folder", shoot2.to_str().unwrap()).unwrap();
    let second = env.run_similarity();
    assert_eq!(second.hashed, 2, "{second:?}");
    assert!(env.db.list_similarity_groups(50).unwrap().is_empty());
    env.db.set_setting("active_folder", env.root.join("shoot").to_str().unwrap()).unwrap();
    let restored = env.db.list_similarity_groups(50).unwrap();
    assert!(restored.iter().any(|g| group_contains(&env.db, g.id, b) && group_contains(&env.db, g.id, c)));

    // Flat frames stay out of regular similarity groups.
    for g in groups.iter().filter(|g| g.group_type == "similar") {
        assert!(
            !(group_contains(&env.db, g.id, d) && group_contains(&env.db, g.id, d2)),
            "flat frames must not weld: {groups:?}"
        );
    }
}

#[test]
fn similarity_group_photos_page_like_the_grid() {
    let env = Env::new(&[
        ("a.jpg", "ramp"),
        ("a2.jpg", "ramp"),
        ("b.jpg", "stripes"),
    ]);
    env.run_similarity();
    let groups = env.db.list_similarity_groups(50).unwrap();
    let g = groups
        .iter()
        .find(|g| g.group_type == "similar" && g.photo_count >= 2)
        .expect("a + a2 must form a group");
    let (page, total) = env
        .db
        .group_photos(g.id, 0, 50, GroupPhotoSort::Chronology)
        .unwrap();
    assert_eq!(total, g.photo_count);
    assert_eq!(page.len(), g.photo_count as usize);
    // Covers are bounded by 4 and always reference real group members.
    assert!(g.cover_photos.len() <= 4);
    for c in &g.cover_photos {
        assert!(group_contains(&env.db, g.id, *c));
    }
    // Empty group pages are a clean (0, 0), not an error.
    let (none, none_total) = env
        .db
        .group_photos(9999, 0, 10, GroupPhotoSort::Chronology)
        .unwrap();
    assert!(none.is_empty() && none_total == 0);
}

#[test]
fn burst_context_preserves_unknowns_and_supports_review_sorts() {
    let env = Env::new(&[
        ("f1.jpg", "ramp"),
        ("f2.jpg", "ramp"),
        ("f3.jpg", "ramp"),
        ("f4.jpg", "ramp"),
        ("f5.jpg", "ramp"),
        ("f6.jpg", "ramp"),
    ]);
    let session_id = env.one_i64("SELECT id FROM sessions ORDER BY id LIMIT 1");
    let ids = env.ids_for(&[
        "f1.jpg", "f2.jpg", "f3.jpg", "f4.jpg", "f5.jpg", "f6.jpg",
    ]);
    for (index, photo_id) in ids.iter().enumerate() {
        env.raw(&format!(
            "UPDATE photos SET capture_datetime = '2026-08-31T10:00:0{index}Z' WHERE id = {photo_id}"
        ));
    }
    env.db
        .replace_similarity_groups_for_session(
            session_id,
            &[("burst:test".into(), "burst".into(), ids.clone())],
        )
        .unwrap();

    let store_eye_state = |photo_id: i64, open_probability: f64| {
        let closed = i64::from(open_probability <= 0.2);
        env.db
            .upsert_faces_with_observations(
                photo_id,
                1,
                2,
                closed,
                if closed == 1 { Some(1.0 - open_probability) } else { None },
                &[FaceObservationInput {
                    appearance_hash: 0xA5A5_A5A5_A5A5_A5A5,
                    left_eye_open_prob: Some(open_probability),
                    right_eye_open_prob: Some(open_probability),
                }],
                Some("2026-01-01T00:00:00Z"),
            )
            .unwrap();
    };
    store_eye_state(ids[0], 0.95);
    store_eye_state(ids[1], 0.05);
    store_eye_state(ids[2], 0.95);
    store_eye_state(ids[3], 0.95);
    // f5 intentionally has no eye measurements, so f4 remains unknown.
    store_eye_state(ids[5], 0.95);
    env.db
        .recompute_possible_blinks_for_session(session_id)
        .unwrap();

    assert_eq!(env.db.get_photo_full(ids[0]).unwrap().possible_blink, None);
    assert_eq!(env.db.get_photo_full(ids[1]).unwrap().possible_blink, Some(true));
    assert_eq!(env.db.get_photo_full(ids[2]).unwrap().possible_blink, Some(false));
    assert_eq!(env.db.get_photo_full(ids[3]).unwrap().possible_blink, None);
    assert_eq!(env.db.get_photo_full(ids[4]).unwrap().possible_blink, None);
    assert_eq!(env.db.get_photo_full(ids[5]).unwrap().possible_blink, None);

    for (index, photo_id) in ids.iter().enumerate() {
        if index == 4 {
            continue;
        }
        env.raw(&format!(
            "UPDATE analysis SET sharpness = {}, highlight_clipping = {}, shadow_clipping = 0 WHERE photo_id = {photo_id}",
            [10, 80, 60, 40, 0, 20][index],
            [8, 6, 4, 2, 0, 0][index]
        ));
    }

    let group = env
        .db
        .list_similarity_groups(50)
        .unwrap()
        .into_iter()
        .find(|group| group.group_type == "burst")
        .unwrap();
    assert_eq!(group.possible_blink_count, 1);
    assert_eq!(group.closed_eye_candidate_count, 1);
    assert_eq!(group.unevaluated_eye_count, 1);

    let sorted_ids = |sort| {
        env.db
            .group_photos(group.id, 0, 50, sort)
            .unwrap()
            .0
            .into_iter()
            .map(|photo| photo.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(sorted_ids(GroupPhotoSort::Chronology), ids);
    assert_eq!(
        sorted_ids(GroupPhotoSort::SharpnessDesc),
        vec![ids[1], ids[2], ids[3], ids[5], ids[0], ids[4]]
    );
    assert_eq!(
        sorted_ids(GroupPhotoSort::ClippingAsc),
        vec![ids[5], ids[3], ids[2], ids[1], ids[0], ids[4]]
    );
    assert_eq!(
        sorted_ids(GroupPhotoSort::EyesOpenFirst),
        vec![ids[0], ids[2], ids[3], ids[5], ids[4], ids[1]]
    );

    let possible = filters::parse_filter(
        r#"{"operator":"AND","conditions":[{"field":"possible_blink","operator":"=","value":true}]}"#,
    )
    .unwrap();
    let (where_sql, params) = filters::build_where(&possible).unwrap();
    let (photos, total) = env.db.photos_where(&where_sql, params, 0, 50).unwrap();
    assert_eq!(total, 1);
    assert_eq!(photos[0].id, ids[1]);

    let not_possible = filters::parse_filter(
        r#"{"operator":"AND","conditions":[{"field":"possible_blink","operator":"=","value":false}]}"#,
    )
    .unwrap();
    let (where_sql, params) = filters::build_where(&not_possible).unwrap();
    let (photos, total) = env.db.photos_where(&where_sql, params, 0, 50).unwrap();
    assert_eq!(total, 1, "NULL context must not be treated as false");
    assert_eq!(photos[0].id, ids[2]);
}

#[test]
fn similarity_cancel_before_start_leaves_no_work() {
    let env = Env::new(&[("a.jpg", "ramp"), ("b.jpg", "stripes")]);
    let progress: Arc<dyn Fn(ProgressPayload) + Send + Sync> =
        Arc::new(|_: ProgressPayload| {});
    let cancel = Arc::new(AtomicBool::new(true)); // cancel before the first file
    let s = similarity::run_similarity(env.db.clone(), progress, cancel).unwrap();
    assert!(s.cancelled, "immediate cancel must be reported");
    assert_eq!(s.hashed, 0);
    // Grouping still completes over (zero) hashes → an empty, consistent set.
    assert_eq!(env.db.list_similarity_groups(50).unwrap().len(), 0);
}
