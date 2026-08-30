//! PhotoGremlin - entry point.
//!
//! Privacy contract: this application makes no network requests. Everything
//! (scanning, analysis, filtering, statistics) runs locally on the user's
//! machine.

pub mod analysis;
pub mod commands;
pub mod contact_sheet;
pub mod database;
pub mod decode;
pub mod error;
pub mod events;
pub mod filesystem;
pub mod filters;
mod logging;
pub mod metadata;
pub mod ml;
pub mod paths;
pub mod scanner;
pub mod similarity;
pub mod state;
pub mod statistics;
pub mod thumbnailer;
pub mod time;

use std::sync::Arc;

use crate::paths::AppPaths;
use tauri::Manager;

pub fn run() {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(run_inner)) {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            logging::write_runtime_error(&error.to_string());
            std::process::exit(1);
        }
        Err(_) => {
            // The hook records the panic and backtrace before unwinding reaches
            // here. This marker makes an abrupt app shutdown easy to identify.
            logging::write_panic_boundary_marker();
            std::process::exit(1);
        }
    }
}

fn run_inner() -> Result<(), tauri::Error> {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let resolver = handle.path();
            let paths = Arc::new(AppPaths::from_resolver(&resolver));

            // Log file must exist before we log much of anything.
            let log_dir = paths.log_dir.clone();
            paths.ensure().expect("could not create app directories");
            logging::init(&log_dir, "photogremlin");
            logging::install_panic_hook(&log_dir);
            tracing::info!(
                version = env!("CARGO_PKG_VERSION"),
                "PhotoGremlin starting (local-only mode)"
            );

            let settings_db = Arc::new(
                crate::database::Db::open(&paths.db_path())
                    .expect("could not open local database"),
            );
            let settings_version = settings_db
                .schema_version()
                .expect("could not inspect settings database version");
            if settings_version > 0 && settings_version < crate::database::CURRENT_SCHEMA_VERSION {
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis();
                let backup = paths
                    .catalog_backups_dir()
                    .join(format!("pre-migration-database-{stamp}.sqlite"));
                settings_db
                    .backup_to(&backup)
                    .expect("could not back up database before migration");
                tracing::info!(from = settings_version, backup = %backup.display(), "database backed up before migration");
            }
            settings_db.migrate().expect("could not migrate settings database");
            settings_db
                .integrity_check()
                .expect("settings database failed integrity check");
            let (catalog, catalog_path) = crate::commands::app::initial_catalog(
                &settings_db,
                &paths,
            )
            .expect("could not open active project catalog");
            let cache_quota = settings_db
                .get_setting(crate::commands::cache::SETTING_CACHE_QUOTA_BYTES)
                .ok()
                .flatten()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(crate::thumbnailer::DEFAULT_CACHE_QUOTA_BYTES);

            app.manage(crate::state::AppState::new(
                settings_db,
                catalog,
                catalog_path,
                paths.clone(),
                Arc::new(crate::thumbnailer::ThumbService::with_quota(
                    paths.thumbnails_dir(),
                    cache_quota,
                )),
            ));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::app_paths,
            commands::db_status,
            commands::log_client_error,
            commands::pick_folder,
            commands::pick_editor_application,
            commands::set_active_folder,
            commands::get_active_folder,
            commands::get_recent_projects,
            commands::remove_recent_project,
            commands::clear_recent_projects,
            commands::close_project,
            commands::open_project,
            commands::catalog_health,
            commands::backup_catalog,
            commands::list_catalog_backups,
            commands::restore_catalog,
            commands::cache_status,
            commands::set_cache_quota,
            commands::clear_cache,
            commands::open_in_file_manager,
            commands::get_dashboard_layout,
            commands::set_dashboard_layout,
            commands::get_editor_config,
            commands::set_editor_config,
            commands::clear_editor_config,
            commands::launch_in_editor,
            commands::start_scan,
            commands::stop_scan,
            commands::start_analysis,
            commands::stop_analysis,
            commands::start_metadata,
            commands::stop_metadata,
            commands::pause_metadata,
            commands::resume_metadata,
            commands::plan_group_rename,
            commands::start_group_rename,
            commands::plan_move_copy,
            commands::start_move_copy,
            commands::plan_trash,
            commands::start_trash,
            commands::stop_operation,
            commands::set_selection,
            commands::set_selections,
            commands::clear_selection,
            commands::clear_selections,
            commands::list_selections,
            commands::update_marks,
            commands::export_contact_sheet,
            commands::stop_export,
            commands::recent_file_ops,
            commands::list_filtered_photos,
            commands::filter_value_options,
            commands::numeric_filter_stats,
            commands::period_stats,
            commands::session_summary,
            commands::compare_sessions,
            commands::list_sessions,
            commands::list_photos,
            commands::review_queue,
            commands::get_review_progress,
            commands::set_review_progress,
            commands::get_photo_full,
            commands::get_thumbnail,
            commands::start_similarity,
            commands::stop_similarity,
            commands::list_similarity_groups,
            commands::group_photos,
            commands::list_collections,
            commands::create_collection,
            commands::rename_collection,
            commands::delete_collection,
            commands::add_to_collection,
            commands::remove_from_collection,
            commands::collection_photos,
            commands::list_saved_views,
            commands::save_view,
            commands::rename_saved_view,
            commands::delete_saved_view,
            commands::saved_view_count,
            commands::ai_status,
            commands::set_ai_enabled,
            commands::start_faces,
            commands::stop_faces,
            commands::start_scene_classification,
            commands::stop_scene_classification,
        ])
        .run(tauri::generate_context!())
}
