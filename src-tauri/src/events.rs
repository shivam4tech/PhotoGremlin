//! IPC event names emitted to the frontend.

pub const SCAN_PROGRESS: &str = "scan-progress";
pub const ANALYSIS_PROGRESS: &str = "analysis-progress";
pub const DB_CHANGED: &str = "db-changed";
pub const OPERATION_PROGRESS: &str = "operation-progress";

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
