use embedded_io_async::ErrorKind;
use thiserror::Error;

/// Errors that can occur during filesystem operations.
#[derive(Debug, Error)]
pub enum FsError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("File not found: {path}")]
    NotFound { path: String },

    #[error("Permission denied: {path}")]
    PermissionDenied { path: String },

    #[error("File already exists: {path}")]
    AlreadyExists { path: String },

    #[error("Invalid path: {reason}")]
    InvalidPath { reason: String },

    #[error("Not a directory: {path}")]
    NotADirectory { path: String },

    #[error("Is a directory: {path}")]
    IsADirectory { path: String },

    #[error("Storage quota exceeded")]
    QuotaExceeded,

    #[error("Operation not supported: {0}")]
    NotSupported(String),

    #[error("Directory not empty: {path}")]
    DirectoryNotEmpty { path: String },

    #[error("File too large")]
    FileTooLarge,

    #[error("Invalid seek position")]
    InvalidSeek,

    #[error("Write cancelled")]
    WriteCancelled,

    #[error("Custom: {0}")]
    Custom(String),
}

impl embedded_io_async::Error for FsError {
    fn kind(&self) -> ErrorKind {
        match self {
            FsError::Io(e) => match e.kind() {
                std::io::ErrorKind::NotFound => ErrorKind::NotFound,
                std::io::ErrorKind::PermissionDenied => ErrorKind::PermissionDenied,
                std::io::ErrorKind::AlreadyExists => ErrorKind::AlreadyExists,
                std::io::ErrorKind::InvalidInput => ErrorKind::InvalidInput,
                std::io::ErrorKind::InvalidData => ErrorKind::InvalidData,
                std::io::ErrorKind::WriteZero => ErrorKind::WriteZero,
                std::io::ErrorKind::BrokenPipe => ErrorKind::BrokenPipe,
                std::io::ErrorKind::ConnectionRefused => ErrorKind::ConnectionRefused,
                std::io::ErrorKind::ConnectionReset => ErrorKind::ConnectionReset,
                std::io::ErrorKind::ConnectionAborted => ErrorKind::ConnectionAborted,
                std::io::ErrorKind::TimedOut => ErrorKind::TimedOut,
                std::io::ErrorKind::Interrupted => ErrorKind::Interrupted,
                _ => ErrorKind::Other,
            },
            FsError::NotFound { .. } => ErrorKind::NotFound,
            FsError::PermissionDenied { .. } => ErrorKind::PermissionDenied,
            FsError::AlreadyExists { .. } => ErrorKind::AlreadyExists,
            FsError::InvalidPath { .. } => ErrorKind::InvalidInput,
            FsError::InvalidSeek => ErrorKind::InvalidInput,
            FsError::QuotaExceeded => ErrorKind::OutOfMemory,
            _ => ErrorKind::Other,
        }
    }
}

/// Result type for filesystem operations.
pub type Result<T> = core::result::Result<T, FsError>;
