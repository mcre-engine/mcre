pub mod error;
pub mod path;
pub mod native;
pub mod web;

pub use error::{FsError, Result};
pub use path::FsPath;

use embedded_io_async::{Read, Seek, Write};

/// Metadata about a file or directory.
#[derive(Debug, Clone)]
pub struct Metadata {
    pub is_file: bool,
    pub is_dir: bool,
    pub size: u64,
}

/// An entry in a directory listing.
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub path: FsPath,
    pub metadata: Metadata,
}

/// Options for opening a file.
#[derive(Debug, Clone, Default)]
pub struct OpenOptions {
    pub read: bool,
    pub write: bool,
    pub create: bool,
    pub truncate: bool,
    pub append: bool,
}

impl OpenOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn read(mut self, read: bool) -> Self {
        self.read = read;
        self
    }

    pub fn write(mut self, write: bool) -> Self {
        self.write = write;
        self
    }

    pub fn create(mut self, create: bool) -> Self {
        self.create = create;
        self
    }

    pub fn truncate(mut self, truncate: bool) -> Self {
        self.truncate = truncate;
        self
    }

    pub fn append(mut self, append: bool) -> Self {
        self.append = append;
        self
    }
}

/// A platform-agnostic filesystem.
///
/// This trait defines the operations available on a filesystem.
/// Implementations provide platform-specific backends (native, web/OPFS).
///
/// # Usage
///
/// The `Fs` object is created platform-specifically and passed to functions
/// that need filesystem operations:
///
/// ```ignore
/// async fn load_resource(fs: &impl Fs, path: &FsPath) -> Result<Vec<u8>> {
///     let mut file = fs.open(path, &OpenOptions::new().read(true)).await?;
///     let mut data = Vec::new();
///     file.read_to_end(&mut data).await?;
///     Ok(data)
/// }
/// ```
#[allow(async_fn_in_trait)]
pub trait Fs: Send + Sync {
    /// The file handle type returned by this filesystem.
    type File: Read<Error = FsError> + Write<Error = FsError> + Seek<Error = FsError> + Send + Sync;

    /// Opens a file with the given options.
    async fn open(&self, path: &FsPath, options: &OpenOptions) -> Result<Self::File>;

    /// Creates a file, overwriting it if it exists.
    async fn create(&self, path: &FsPath) -> Result<Self::File> {
        self.open(
            path,
            &OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true),
        )
        .await
    }

    /// Opens a file for reading.
    async fn open_read(&self, path: &FsPath) -> Result<Self::File> {
        self.open(path, &OpenOptions::new().read(true)).await
    }

    /// Opens a file for writing, creating it if it doesn't exist.
    async fn open_write(&self, path: &FsPath) -> Result<Self::File> {
        self.open(
            path,
            &OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true),
        )
        .await
    }

    /// Creates a directory and all parent directories.
    async fn create_dir_all(&self, path: &FsPath) -> Result<()>;

    /// Removes a file.
    async fn remove_file(&self, path: &FsPath) -> Result<()>;

    /// Removes a directory and all its contents.
    async fn remove_dir_all(&self, path: &FsPath) -> Result<()>;

    /// Renames or moves a file or directory.
    async fn rename(&self, from: &FsPath, to: &FsPath) -> Result<()>;

    /// Returns metadata for the file or directory at the given path.
    async fn metadata(&self, path: &FsPath) -> Result<Metadata>;

    /// Returns true if the path exists.
    async fn exists(&self, path: &FsPath) -> bool {
        self.metadata(path).await.is_ok()
    }

    /// Returns true if the path is a file.
    async fn is_file(&self, path: &FsPath) -> bool {
        self.metadata(path)
            .await
            .map(|m| m.is_file)
            .unwrap_or(false)
    }

    /// Returns true if the path is a directory.
    async fn is_dir(&self, path: &FsPath) -> bool {
        self.metadata(path)
            .await
            .map(|m| m.is_dir)
            .unwrap_or(false)
    }

    /// Reads the entire contents of a file into a byte vector.
    async fn read(&self, path: &FsPath) -> Result<Vec<u8>> {
        let mut file = self.open_read(path).await?;
        let mut data = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            let n = file.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            data.extend_from_slice(&buf[..n]);
        }
        Ok(data)
    }

    /// Reads the entire contents of a file into a string.
    async fn read_to_string(&self, path: &FsPath) -> Result<String> {
        let data = self.read(path).await?;
        String::from_utf8(data).map_err(|e| FsError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("File is not valid UTF-8: {}", e),
        )))
    }

    /// Writes a byte slice to a file, creating it if it doesn't exist.
    async fn write(&self, path: &FsPath, data: &[u8]) -> Result<()> {
        let mut file = self.open_write(path).await?;
        let mut written = 0;
        while written < data.len() {
            let n = file.write(&data[written..]).await?;
            written += n;
        }
        file.flush().await?;
        Ok(())
    }

    /// Writes a string to a file, creating it if it doesn't exist.
    async fn write_string(&self, path: &FsPath, data: &str) -> Result<()> {
        self.write(path, data.as_bytes()).await
    }

    /// Lists the contents of a directory.
    async fn read_dir(&self, path: &FsPath) -> Result<Vec<DirEntry>>;

    /// Copies a file from one location to another.
    async fn copy(&self, from: &FsPath, to: &FsPath) -> Result<()> {
        let data = self.read(from).await?;
        self.write(to, &data).await?;
        Ok(())
    }
}
