use crate::{FsError, Result};
use embedded_io_async::{ErrorType, Read, Seek, Write};
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

/// A file handle for the native filesystem.
pub struct NativeFile {
    file: tokio::fs::File,
    path: PathBuf,
}

impl NativeFile {
    pub(crate) fn new(file: tokio::fs::File, path: PathBuf) -> Self {
        Self { file, path }
    }

    /// Returns the path of this file.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Consumes this handle and returns the inner `tokio::fs::File`.
    pub fn into_inner(self) -> tokio::fs::File {
        self.file
    }

    /// Returns a reference to the inner `tokio::fs::File`.
    pub fn inner(&self) -> &tokio::fs::File {
        &self.file
    }

    /// Returns a mutable reference to the inner `tokio::fs::File`.
    pub fn inner_mut(&mut self) -> &mut tokio::fs::File {
        &mut self.file
    }

    /// Flushes all internal buffers to disk.
    pub async fn sync_all(&self) -> Result<()> {
        self.file.sync_all().await.map_err(FsError::Io)
    }

    /// Similar to `sync_all`, but may not flush file metadata.
    pub async fn sync_data(&self) -> Result<()> {
        self.file.sync_data().await.map_err(FsError::Io)
    }

    /// Truncates or extends the underlying file.
    pub async fn set_len(&self, size: u64) -> Result<()> {
        self.file.set_len(size).await.map_err(FsError::Io)
    }

    /// Queries metadata about the underlying file.
    pub async fn metadata(&self) -> Result<std::fs::Metadata> {
        self.file.metadata().await.map_err(FsError::Io)
    }

    /// Creates a new `OwnedFd` from this file handle.
    #[cfg(unix)]
    pub async fn into_std(self) -> std::fs::File {
        self.file.into_std().await
    }
}

impl ErrorType for NativeFile {
    type Error = FsError;
}

impl Read for NativeFile {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.file.read(buf).await.map_err(FsError::Io)
    }

    async fn read_exact(
        &mut self,
        buf: &mut [u8],
    ) -> core::result::Result<(), embedded_io_async::ReadExactError<FsError>> {
        self.file.read_exact(buf).await.map(|_| ()).map_err(|e| {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                embedded_io_async::ReadExactError::UnexpectedEof
            } else {
                embedded_io_async::ReadExactError::Other(FsError::Io(e))
            }
        })
    }
}

impl Write for NativeFile {
    async fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.file.write(buf).await.map_err(FsError::Io)
    }

    async fn flush(&mut self) -> Result<()> {
        self.file.flush().await.map_err(FsError::Io)
    }
}

impl Seek for NativeFile {
    async fn seek(&mut self, pos: embedded_io_async::SeekFrom) -> Result<u64> {
        let std_pos = match pos {
            embedded_io_async::SeekFrom::Start(n) => std::io::SeekFrom::Start(n),
            embedded_io_async::SeekFrom::Current(n) => std::io::SeekFrom::Current(n),
            embedded_io_async::SeekFrom::End(n) => std::io::SeekFrom::End(n),
        };
        self.file.seek(std_pos).await.map_err(FsError::Io)
    }
}
