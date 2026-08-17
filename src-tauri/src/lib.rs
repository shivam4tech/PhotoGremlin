//! PhotoGremlin - entry point.
//!
//! Privacy contract: this application makes no network requests. Everything
//! (scanning, analysis, filtering, statistics) runs locally on the user's
//! machine.

pub mod analysis;
pub mod commands;
pub mod database;
pub mod error;
pub mod events;
pub mod filters;
mod logging;
pub mod metadata;
pub mod paths;
pub mod scanner;
pub mod state;
pub mod thumbnailer;
pub mod time;

use std::sync::{Arc, Mutex};

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

            app.manage(crate::state::AppState {
                db,
                paths: paths.clone(),
                scan: Arc::new(Mutex::new(None)),
                analysis: Arc::new(Mutex::new(None)),
                metadata: Arc::new(Mutex::new(None)),
                thumb: Arc::new(crate::thumbnailer::ThumbService::new(
                    paths.thumbnails_dir(),
                )),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::app_paths,
            commands::db_status,
            commands::pick_folder,
            commands::set_active_folder,
            commands::get_active_folder,
            commands::start_scan,
            commands::stop_scan,
            commands::start_analysis,
            commands::stop_analysis,
            commands::start_metadata,
            commands::stop_metadata,
            commands::list_filtered_photos,
            commands::list_sessions,
            commands::list_photos,
            commands::get_photo_full,
            commands::get_thumbnail,
        ])
        .run(tauri::generate_context!())
        .expect("error while running PhotoGremlin");
}
