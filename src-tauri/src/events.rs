//! IPC event names emitted to the frontend.

pub const SCAN_PROGRESS: &str = "scan-progress";
pub const SCAN_COMPLETE: &str = "scan-complete";
pub const ANALYSIS_PROGRESS: &str = "analysis-progress";
pub const ANALYSIS_COMPLETE: &str = "analysis-complete";
pub const METADATA_PROGRESS: &str = "metadata-progress";
pub const METADATA_COMPLETE: &str = "metadata-complete";
pub const DB_CHANGED: &str = "db-changed";
pub const OPERATION_PROGRESS: &str = "operation-progress";
pub const OPERATION_COMPLETE: &str = "operation-complete";
pub const SIMILARITY_PROGRESS: &str = "similarity-progress";
pub const SIMILARITY_COMPLETE: &str = "similarity-complete";
pub const FACES_PROGRESS: &str = "faces-progress";
pub const FACES_COMPLETE: &str = "faces-complete";
/// Scene-classification pass (Sprint 18).
pub const SCENES_PROGRESS: &str = "scenes-progress";
pub const SCENES_COMPLETE: &str = "scenes-complete";
pub const CONTACT_SHEET_PROGRESS: &str = "contact-sheet-progress";
pub const CONTACT_SHEET_COMPLETE: &str = "contact-sheet-complete";

/// Payload shape shared by scan/analysis/operation progress events.
#[derive(Clone, serde::Serialize)]
pub struct ProgressPayload {
    pub total: usize,
    pub done: usize,
    pub stage: String,
    pub current: Option<String>,
}

impl ProgressPayload {
    pub fn new(total: usize, done: usize, stage: impl Into<String>) -> Self {
        Self {
            total,
            done,
            stage: stage.into(),
            current: None,
        }
    }

    pub fn with_current(mut self, current: impl Into<String>) -> Self {
        self.current = Some(current.into());
        self
    }
}

/// Emitted when a contact-sheet export finishes (Sprint 14). `files` are
/// the absolute paths of the written pages (possibly a prefix of what was
/// requested when `cancelled`).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ContactSheetCompletePayload {
    pub files: Vec<String>,
    pub error: Option<String>,
    pub cancelled: bool,
}
