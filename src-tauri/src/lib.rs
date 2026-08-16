//! PhotoGremlin - entry point.
//!
//! Privacy contract: this application makes no network requests. Everything
//! (scanning, analysis, filtering, statistics) runs locally on the user's
//! machine.

pub mod commands;
pub mod database;
pub mod error;
pub mod events;
mod logging;
pub mod paths;
pub mod state;
pub mod time;

use std::sync::Arc;

use crate::paths::AppPaths;
use tauri::Manager;

pub fn run() {
    let _app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let resolver = handle.path();
            let paths = Arc::new(AppPaths::from_resolver(&resolver));

            // Log file must exist before we log much of anything.
            let log_dir = paths.log_dir.clone();
            paths.ensure().expect("could not create app directories");
            logging::init(&log_dir, "photogremlin");
            tracing::info!(
                version = env!("CARGO_PKG_VERSION"),
                "PhotoGremlin starting (local-only mode)"
            );

            let db = Arc::new(
                crate::database::Db::open(&paths.db_path())
                    .expect("could not open local database"),
            );
            db.migrate().expect("could not migrate schema");

            app.manage(crate::state::AppState { db, paths });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::app_paths,
            commands::db_status,
            commands::pick_folder,
            commands::set_active_folder,
            commands::get_active_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running PhotoGremlin");
}
