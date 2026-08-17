//! Tauri commands (the IPC surface). Grouped by domain.

pub mod analysis;
pub mod app;
pub mod fileops;
pub mod filters;
pub mod metadata;
pub mod photos;
pub mod scan;
pub mod stats;

pub use analysis::*;
pub use app::*;
pub use fileops::*;
pub use filters::*;
pub use metadata::*;
pub use photos::*;
pub use scan::*;
pub use stats::*;
