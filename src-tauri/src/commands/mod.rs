//! Tauri commands (the IPC surface). Grouped by domain.

pub mod app;
pub mod scan;

pub use app::*;
pub use scan::*;
