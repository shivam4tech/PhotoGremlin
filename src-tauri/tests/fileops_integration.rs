//! Integration test: real temp folders → plan + execute rename/move/copy/
//! trash on real files → verify filesystem, DB sync, and the audit log.
//!
//! This exercises the exact pipeline that ships (FILE_OPERATIONS.md's
//! universal protocol) against real bytes on disk: collision detection,
//! in-plan rename abort, staged copy verification, DB path updates, trash
//! (OS trash on Linux), per-item partial failure, and the audit trail.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use photogremlin_lib::database::{Db, PhotoUpsert};
use photogremlin_lib::events::ProgressPayload;
use photogremlin_lib::filesystem::{
    self, CollisionPolicy, OpKind,
};

fn plain_jpeg(w: u32, h: u32) -> Vec<u8> {
    let img = image::ImageBuffer::from_pixel(w, h, image::Rgb([90, 120, 140]));
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

/// A scratch root containing a shoot dir (with N real JPEGs), a move target
/// dir, a copy target dir, and a temp DB. Each test gets its own process+pid
/// root so parallel tests never collide.
struct Env {
    root: PathBuf,
    shoot: PathBuf,
    move_dest: PathBuf,
    copy_dest: PathBuf,
    db: Db,
}

impl Env {
    fn new(shoot_files: &[&str]) -> Env {
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "pg_fileops_it_{n}_{}_{}",
            shoot_files.len(),
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let shoot = root.join("shoot");
        let move_dest = root.join("moved");
        let copy_dest = root.join("copied");
        for d in [&shoot, &move_dest, &copy_dest] {
            std::fs::create_dir_all(d).unwrap();
        }
        for name in shoot_files.iter() {
            std::fs::write(shoot.join(*name), plain_jpeg(64, 48)).unwrap();
        }
        let db_path = root.join("db.sqlite");
        let _ = std::fs::remove_file(&db_path);
        let db = Db::open(&db_path).unwrap();
        db.migrate().unwrap();
        let session = db.upsert_session("Shoot", Some(shoot.to_str().unwrap())).unwrap();
        for name in shoot_files {
            let p = shoot.join(name);
            db.upsert_photo(&PhotoUpsert {
                path: p.to_string_lossy().into_owned(),
                filename: name.to_string(),
                extension: "jpg".into(),
                size_bytes: Some(123),
                width: Some(64),
                height: Some(48),
                orientation: Some("landscape".into()),
                session_id: Some(session),
                file_mtime: None,
            })
            .unwrap();
        }
        Env {
            root,
            shoot,
            move_dest,
            copy_dest,
            db,
        }
    }

    /// (id, path) for a filename.
    fn photos(&self) -> Vec<(i64, String, String)> {
        let conn = self.db.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, path, filename FROM photos ORDER BY filename")
            .unwrap();
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap();
        rows.collect::<Result<Vec<_>, _>>().unwrap()
    }

    fn ids_for(&self, names: &[&str]) -> Vec<i64> {
        self.photos()
            .into_iter()
            .filter(|(_, _, f)| names.contains(&f.as_str()))
            .map(|(id, _, _)| id)
            .collect()
    }

    fn path_for(&self, name: &str) -> String {
        self.photos()
            .into_iter()
            .find(|(_, _, f)| f == name)
            .map(|(_, p, _)| p)
            .unwrap()
    }

    fn op_rows(&self) -> Vec<(String, String, String)> {
        let conn = self.db.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT op_type, source_path, status FROM file_operations ORDER BY id",
            )
            .unwrap();
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap();
        rows.collect::<Result<Vec<_>, _>>().unwrap()
    }

    fn drop(self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn run(db: &Db, plan: &filesystem::FileOpPlan, cancel: &AtomicBool) -> filesystem::OperationSummary {
    let mut progress = |_: ProgressPayload| {};
    filesystem::run_operation(db, plan, &mut progress, cancel).unwrap()
}

#[test]
fn group_renames_files_and_syncs_db_and_audit() {
    let env = Env::new(&["a.jpg", "b.jpg", "c.jpg"]);
    let ids = env.ids_for(&["a.jpg", "b.jpg", "c.jpg"]);
    let plan = filesystem::plan_rename(&env.db, &ids, "{date}_{name}_{sequence}", "Wedding").unwrap();
    assert!(!plan.aborted);
    assert!(plan.items.iter().all(|i| i.ok), "preview should be all-ok");

    let s = run(&env.db, &plan, &AtomicBool::new(false));
    assert_eq!(s.succeeded, 3);
    assert_eq!(s.failed, 0);

    // Files actually renamed on disk (dates fall back to indexed_at → today).
    let renamed: Vec<String> = std::fs::read_dir(&env.shoot)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(renamed.len(), 3);
    for r in &renamed {
        assert!(r.ends_with(".jpg") && r.contains("_Wedding_"), "{r}");
        assert!(!r.starts_with(['a', 'b', 'c']), "old name must be gone: {r}");
    }
    // Sequences are ascending in capture/file order.
    assert!(renamed.iter().any(|r| r.ends_with("_001.jpg")));
    assert!(renamed.iter().any(|r| r.ends_with("_003.jpg")));

    // DB follows the files: every row's path points at an existing file and
    // the old source paths are gone.
    for (id, path, _f) in env.photos() {
        assert!(Path::new(&path).exists(), "DB row dangles: {path}");
        let _ = id;
    }

    // Audit log records each rename as done.
    let ops = env.op_rows();
    assert_eq!(ops.len(), 3);
    for (op, _src, status) in &ops {
        assert_eq!(op, "rename");
        assert_eq!(status, "done");
    }
    env.drop();
}

#[test]
fn rename_in_plan_collision_aborts_with_itemized_report() {
    let env = Env::new(&["a.jpg", "b.jpg"]);
    let ids = env.ids_for(&["a.jpg", "b.jpg"]);
    // A template with no distinguishing token maps both files to one name.
    let plan = filesystem::plan_rename(&env.db, &ids, "fixed_name", "X").unwrap();
    assert!(plan.aborted, "two sources onto one name must abort");
    assert!(plan.items.iter().all(|i| !i.ok));
    assert!(plan
        .items
        .iter()
        .all(|i| i.note.as_deref().unwrap_or("").contains("aborted")));
    // Aborted plans must not be executed.
    assert!(filesystem::run_operation(
        &env.db,
        &plan,
        &mut |_| {},
        &AtomicBool::new(false),
    )
    .unwrap()
    .succeeded
        == 0);
    // Nothing renamed.
    assert!(env.shoot.join("a.jpg").exists());
    env.drop();
}

#[test]
fn rename_onto_existing_file_blocks_that_item_only() {
    let env = Env::new(&["a.jpg", "b.jpg"]);
    // Force a on-disk collision: a.jpg would rename to b.jpg's spot.
    let ids = env.ids_for(&["a.jpg"]);
    let plan = filesystem::plan_rename(&env.db, &ids, "b", "X").unwrap();
    assert!(!plan.aborted);
    assert_eq!(plan.items[0].note.as_deref(), Some("ALREADY EXISTS"));
    assert!(!plan.items[0].ok);
    env.drop();
}

#[test]
fn move_updates_fs_path_db_and_audit() {
    let env = Env::new(&["a.jpg"]);
    let ids = env.ids_for(&["a.jpg"]);
    let src = env.path_for("a.jpg"); // original path, captured before the move
    let plan =
        filesystem::plan_move_copy(&env.db, &ids, &env.move_dest, OpKind::Move, CollisionPolicy::Skip).unwrap();
    assert!(plan.items.iter().all(|i| i.ok));

    let s = run(&env.db, &plan, &AtomicBool::new(false));
    assert_eq!(s.succeeded, 1);

    // File moved (source gone, dest present), DB + audit updated.
    assert!(!env.shoot.join("a.jpg").exists());
    assert!(env.move_dest.join("a.jpg").exists());
    assert_eq!(
        env.path_for("a.jpg"),
        env.move_dest.join("a.jpg").to_string_lossy()
    );
    // Audit: one row, op=move, source = the original path, done.
    assert_eq!(
        env.op_rows(),
        vec![("move".to_string(), src, "done".to_string())]
    );
    env.drop();
}

#[test]
fn copy_dups_file_keeps_original_and_indexes_copy() {
    let env = Env::new(&["a.jpg"]);
    let ids = env.ids_for(&["a.jpg"]);
    let plan =
        filesystem::plan_move_copy(&env.db, &ids, &env.copy_dest, OpKind::Copy, CollisionPolicy::Skip).unwrap();
    assert!(plan.items.iter().all(|i| i.ok));

    let s = run(&env.db, &plan, &AtomicBool::new(false));
    assert_eq!(s.succeeded, 1);

    // Original intact, copy present, both now in the DB (2 rows).
    assert!(env.shoot.join("a.jpg").exists());
    assert!(env.copy_dest.join("a.jpg").exists());
    let all = env.photos();
    assert_eq!(all.len(), 2);
    assert!(all.iter().any(|(_, p, _)| p == &env.copy_dest.join("a.jpg").to_string_lossy()));
    env.drop();
}

#[test]
fn collision_policy_skip_blocks_and_avoid_renames() {
    let env = Env::new(&["a.jpg"]);
    // Pre-create the destination so a collision exists.
    std::fs::write(env.copy_dest.join("a.jpg"), plain_jpeg(8, 8)).unwrap();

    let ids = env.ids_for(&["a.jpg"]);
    let skip =
        filesystem::plan_move_copy(&env.db, &ids, &env.copy_dest, OpKind::Copy, CollisionPolicy::Skip).unwrap();
    assert!(!skip.items[0].ok);
    assert!(skip.items[0].note.as_deref().unwrap_or("").contains("ALREADY EXISTS"));

    let avoid = filesystem::plan_move_copy(
        &env.db,
        &ids,
        &env.copy_dest,
        OpKind::Copy,
        CollisionPolicy::AvoidByRenaming,
    )
    .unwrap();
    assert!(avoid.items[0].ok);
    assert_eq!(
        avoid.items[0].destination.as_deref().unwrap(),
        env.copy_dest.join("a-1.jpg").to_string_lossy()
    );
    env.drop();
}

#[test]
fn partial_failure_keeps_successes_and_reports_reasons() {
    let env = Env::new(&["keep.jpg", "gone.jpg"]);
    // Remove one source before executing so it can't be moved.
    std::fs::remove_file(env.shoot.join("gone.jpg")).unwrap();

    let ids = env.ids_for(&["keep.jpg", "gone.jpg"]);
    let plan =
        filesystem::plan_move_copy(&env.db, &ids, &env.move_dest, OpKind::Move, CollisionPolicy::Skip).unwrap();
    // gone.jpg is flagged at plan time as missing.
    let s = run(&env.db, &plan, &AtomicBool::new(false));
    assert_eq!(s.succeeded, 1);
    // The missing item is skipped (not a hard failure of the whole op).
    assert!(s
        .items
        .iter()
        .any(|i| i.status == "skipped" && i.source.contains("gone.jpg")));
    // The good one actually moved.
    assert!(env.move_dest.join("keep.jpg").exists());
    env.drop();
}

#[test]
fn trash_moves_to_os_trash_and_removes_db_row() {
    if !cfg!(target_os = "linux") {
        eprintln!("trash test is Linux-only in v0.1; skipping");
        return;
    }
    let env = Env::new(&["t.jpg"]);
    let ids = env.ids_for(&["t.jpg"]);
    let plan = filesystem::plan_trash(&env.db, &ids).unwrap();
    assert!(plan.destructive);
    assert!(plan.items.iter().all(|i| i.ok));

    let s = run(&env.db, &plan, &AtomicBool::new(false));
    assert_eq!(s.succeeded, 1);

    // File no longer in the shoot, present in the OS trash `files` dir.
    assert!(!env.shoot.join("t.jpg").exists());
    let trash_files = trash_files_dir();
    assert!(trash_files.join("t.jpg").exists(), "expected t.jpg in trash {trash_files:?}");
    // DB row + audit entry for the trash.
    assert!(env.ids_for(&["t.jpg"]).is_empty(), "DB row should be removed");
    let ops = env.op_rows();
    assert!(ops.iter().any(|(op, _src, st)| op == "trash" && st == "done"));
    // Clean up the trashed file + its metadata so we don't litter the trash.
    let _ = std::fs::remove_file(trash_files.join("t.jpg"));
    let trash_info = trash_files.join("..").join("info");
    let _ = std::fs::remove_file(trash_info.join("t.jpg.trashinfo"));
    env.drop();
}

#[cfg(target_os = "linux")]
fn trash_files_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute() && !p.as_os_str().is_empty())
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".local/share")))
        .map(|d| d.join("Trash/files"))
        .unwrap()
}

#[test]
fn cancel_before_start_processes_nothing() {
    let env = Env::new(&["a.jpg", "b.jpg"]);
    let ids = env.ids_for(&["a.jpg", "b.jpg"]);
    let plan = filesystem::plan_rename(&env.db, &ids, "c_{sequence}", "X").unwrap();
    let cancel = AtomicBool::new(true);
    let s = run(&env.db, &plan, &cancel);
    assert!(s.cancelled);
    assert_eq!(s.succeeded, 0);
    assert!(env.shoot.join("a.jpg").exists());
    let _ = Ordering::Relaxed;
    env.drop();
}
