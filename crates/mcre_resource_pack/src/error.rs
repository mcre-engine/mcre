use core::{error, fmt};

use mcre_fs::FsError;
use zip::result::ZipError;

pub type PackResult<T> = Result<T, PackError>;

#[derive(Debug)]
pub enum PackError {
    Fs(FsError),
    Archive(ZipError),
    Download(reqwest::Error),
}

impl From<FsError> for PackError {
    fn from(err: FsError) -> Self {
        Self::Fs(err)
    }
}

impl From<ZipError> for PackError {
    fn from(err: ZipError) -> Self {
        Self::Archive(err)
    }
}

impl From<reqwest::Error> for PackError {
    fn from(err: reqwest::Error) -> Self {
        Self::Download(err)
    }
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackError::Fs(fs_error) => write!(f, "FS Error: {}", fs_error),
            PackError::Archive(zip_error) => write!(f, "Archive Error: {}", zip_error),
            PackError::Download(error) => write!(f, "Download Error: {}", error),
        }
    }
}

impl error::Error for PackError {}
