//! Tauri commands (the IPC surface). Grouped by domain.

pub mod analysis;
pub mod app;
pub mod photos;
pub mod scan;

pub use analysis::*;
pub use app::*;
pub use photos::*;
pub use scan::*;
