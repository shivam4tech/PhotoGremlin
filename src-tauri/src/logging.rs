//! Local logging only. No telemetry, ever.

use std::backtrace::Backtrace;
use std::fs::OpenOptions;
use std::io::Write;
use std::panic;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

static PANIC_LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

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

/// Add a synchronous, file-backed panic report before any app work that can
/// fail. Panic hooks still execute for aborting release builds, unlike an
/// unwind-only catch boundary.
pub fn install_panic_hook(log_dir: &Path) {
    if PANIC_LOG_PATH.set(log_dir.join("photogremlin.crash.log")).is_err() {
        return;
    }

    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|location| format!("{}:{}:{}", location.file(), location.line(), location.column()))
            .unwrap_or_else(|| "unknown location".to_string());
        let payload = if let Some(message) = info.payload().downcast_ref::<&str>() {
            (*message).to_string()
        } else if let Some(message) = info.payload().downcast_ref::<String>() {
            message.clone()
        } else {
            "non-string panic payload".to_string()
        };
        append_crash_report(&format!(
            "Rust panic at {location}: {payload}\nBacktrace:\n{}\n",
            Backtrace::force_capture()
        ));
        previous(info);
    }));
}

pub fn write_panic_boundary_marker() {
    append_crash_report("Application exited after a Rust panic was caught at the top-level boundary.\n");
}

pub fn write_runtime_error(error: &str) {
    append_crash_report(&format!("Tauri runtime exited with an error: {error}\n"));
}

fn append_crash_report(report: &str) {
    let Some(path) = PANIC_LOG_PATH.get() else {
        return;
    };
    append_report_to(path, report);
}

fn append_report_to(path: &Path, report: &str) {
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let _ = writeln!(file, "--- {} ---\n{report}", crate::time::now_utc());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crash_report_is_written_to_a_local_file() {
        let path = std::env::temp_dir().join(format!(
            "photogremlin-crash-log-{}-{}.log",
            std::process::id(),
            crate::time::now_utc().replace(':', "-")
        ));
        append_report_to(&path, "test report");

        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("test report"));
        std::fs::remove_file(path).unwrap();
    }
}
