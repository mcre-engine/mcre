use thiserror::Error;

#[derive(Debug, Error)]
pub enum ResourcePackError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Filesystem error: {0}")]
    Fs(#[from] mcre_fs::FsError),

    #[error("JSON deserialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Invalid pack format: {0}")]
    InvalidPackFormat(String),

    #[error("Unsupported pack format version: {0}")]
    UnsupportedFormatVersion(u32),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Model resolution error: {0}")]
    ModelResolution(String),

    #[error("Blockstate not found for block: {0}")]
    BlockstateNotFound(String),

    #[error("Block model not found: {0}")]
    ModelNotFound(String),

    #[error("Download error: {0}")]
    Download(String),
}

pub type Result<T> = core::result::Result<T, ResourcePackError>;
