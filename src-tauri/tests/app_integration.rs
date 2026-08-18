//! Integration test: active-folder persistence across "app restarts" and
//! stale-path reclamation. The folder the user last opened is stored in
//! app_settings and must come back on the next start (so the dashboard
//! never forces the user to open the same folder again); if the folder was
//! deleted or moved outside the app in the meantime, it must be forgotten
//! instead of resurrected.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use photogremlin_lib::commands::resolve_active_folder;
use photogremlin_lib::database::Db;

struct Env {
    root: PathBuf,
    folder: PathBuf,
    db: Db,
    db_path: PathBuf,
}

impl Env {
    fn new() -> Env {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "pg_app_it_{n}_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let folder = root.join("photo_folder");
        std::fs::create_dir_all(&folder).unwrap();
        let db_path = root.join("data").join("database.sqlite");
        let db = Db::open(&db_path).unwrap();
        db.migrate().unwrap();
        Env {
            root,
            folder,
            db,
            db_path,
        }
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn no_folder_stored_means_none() {
    let e = Env::new();
    assert_eq!(resolve_active_folder(&e.db).unwrap(), None);
}

#[test]
fn stored_folder_survives_restarts_until_the_path_disappears() {
    let e = Env::new();
    // "Open" the folder (what set_active_folder persists).
    e.db
        .set_setting("active_folder", &e.folder.display().to_string())
        .unwrap();

    // Fresh start of the app: the folder is restored.
    let again = Db::open(&e.db_path).unwrap();
    assert_eq!(
        resolve_active_folder(&again).unwrap(),
        Some(e.folder.display().to_string())
    );
    drop(again);

    // Folder deleted outside the app: the next start forgets it, and the
    // forgetting is persisted (a second start sees the same state).
    std::fs::remove_dir_all(&e.folder).unwrap();
    assert_eq!(resolve_active_folder(&e.db).unwrap(), None);
    assert_eq!(resolve_active_folder(&e.db).unwrap(), None);
    assert_eq!(e.db.get_setting("active_folder").unwrap(), None);
}

#[test]
fn nonexistent_folder_is_never_acceptable_as_active() {
    let e = Env::new();
    let ghost = e.root.join("never_existed");
    e.db
        .set_setting("active_folder", &ghost.display().to_string())
        .unwrap();
    assert_eq!(resolve_active_folder(&e.db).unwrap(), None);
}
