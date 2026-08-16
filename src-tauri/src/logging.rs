//! Local logging only. No telemetry, ever.

use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

pub fn init(log_dir: &std::path::Path, _log_name: &str) {
    let _ = std::fs::create_dir_all(log_dir);
    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("photogremlin")
        .filename_suffix("log")
        .build(log_dir)
        .expect("rolling file appender must init");

    let (file_writer, _guard) = tracing_appender::non_blocking(file_appender);
    // Keep the guard alive for the process lifetime: leak it intentionally.
    std::mem::forget(_guard);

    let file_layer = fmt::layer()
        .with_writer(file_writer)
        .with_ansi(false)
        .with_target(true);

    let console_layer = fmt::layer()
        .with_writer(std::io::stdout)
        .with_target(false);

    let filter = EnvFilter::try_new(
        std::env::var("PHOTOGREMLIN_LOG")
            .unwrap_or_else(|_| "info,photogremlin_lib=debug,rusqlite=warn".into()),
    )
    .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(console_layer)
        .init();

    tracing::info!(log_dir = %log_dir.display(), "logging initialised (local only)");
}
