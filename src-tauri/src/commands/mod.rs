//! Tauri commands (the IPC surface). Grouped by domain.

pub mod ai;
pub mod analysis;
pub mod app;
pub mod collections;
pub mod fileops;
pub mod filters;
pub mod metadata;
pub mod photos;
pub mod scan;
pub mod similarity;
pub mod stats;
pub mod views;

pub use ai::*;
pub use analysis::*;
pub use app::*;
pub use collections::*;
pub use fileops::*;
pub use filters::*;
pub use metadata::*;
pub use photos::*;
pub use scan::*;
pub use similarity::*;
pub use stats::*;
pub use views::*;
