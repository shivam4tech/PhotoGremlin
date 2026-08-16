//! Application-wide error type.
//!
//! Errors surface to the UI as friendly `Display` messages; detailed context
//! is recorded in the local log via `tracing`.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Could not open the database. {0}")]
    Database(String),

    #[error("Could not read {target}. {reason}")]
    FileMissing { target: String, reason: String },

    #[error("Permission denied for {target}. {reason}")]
    PermissionDenied { target: String, reason: String },

    #[error("Unsupported image format: {path}")]
    UnsupportedFormat { path: String },

    #[error("Could not read image: {path}. {reason}")]
    ImageRead { path: String, reason: String },

    #[error("{message}")]
    Validation { message: String },

    #[error("{message}")]
    Operation { message: String },

    #[error(transparent)]
    Other(#[from] Box<other::OtherError>),
}

mod other {
    use thiserror::Error;

    #[derive(Debug, Error)]
    #[error("Something went wrong. {detail}")]
    pub struct OtherError {
        pub detail: String,
    }
}

impl AppError {
    pub fn io(io: std::io::Error, target: impl Into<String>) -> Self {
        let target = target.into();
        let reason = io.to_string();
        tracing::error!(%target, %reason, "io error");
        if io.kind() == std::io::ErrorKind::NotFound {
            AppError::FileMissing { target, reason }
        } else if io.kind() == std::io::ErrorKind::PermissionDenied {
            AppError::PermissionDenied { target, reason }
        } else {
            AppError::Operation {
                message: format!("Could not access {target}. {reason}"),
            }
        }
    }

    pub fn validation(message: impl Into<String>) -> Self {
        AppError::Validation {
            message: message.into(),
        }
    }

    pub fn operation(message: impl Into<String>) -> Self {
        AppError::Operation {
            message: message.into(),
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;

/// Tauri requires command error types to convert into `InvokeError`.
/// The user sees `Display`; details live in the log.
impl From<AppError> for tauri::ipc::InvokeError {
    fn from(e: AppError) -> Self {
        tauri::ipc::InvokeError::from(e.to_string())
    }
}
