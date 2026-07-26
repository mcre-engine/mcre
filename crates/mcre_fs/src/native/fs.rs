use super::{NativeFile, app_data_dir};
use crate::{DirEntry, Fs, FsPath, Metadata, OpenOptions, Result};
use std::path::PathBuf;

/// A filesystem backed by the native OS filesystem (tokio::fs).
///
/// # Examples
///
/// ```ignore
/// use mcre_fs::{native::NativeFs, FsPath, OpenOptions};
///
/// // Create an FS rooted at the application's data directory
/// let fs = NativeFs::new_app_root("com.mcre", "MCRE").await?;
///
/// // Read a file
/// let data = fs.read(&FsPath::from("settings.json")).await?;
/// ```
pub struct NativeFs {
    root: PathBuf,
}

impl NativeFs {
    /// Creates a new `NativeFs` rooted at the application's data directory.
    ///
    /// The root path is determined by the OS:
    /// - macOS: `~/Library/Application Support/<app_name>`
    /// - Linux: `~/.local/share/<app_name>`
    /// - Windows: `%APPDATA%\<app_name>`
    pub async fn new_app_root(app_name: &str) -> Result<Self> {
        let root = app_data_dir(app_name)?;
        tokio::fs::create_dir_all(&root).await?;
        Ok(Self { root })
    }

    /// Creates a new `NativeFs` rooted at the given path.
    ///
    /// The path is resolved relative to the current working directory.
    pub async fn new_custom_path(path: impl Into<PathBuf>) -> Result<Self> {
        let root = path.into();
        tokio::fs::create_dir_all(&root).await?;
        Ok(Self { root })
    }

    /// Creates a new `NativeFs` rooted at the given absolute path.
    ///
    /// Returns an error if the path is not absolute.
    pub async fn new_absolute(path: PathBuf) -> Result<Self> {
        if !path.is_absolute() {
            return Err(crate::FsError::InvalidPath {
                reason: "Path must be absolute".into(),
            });
        }
        tokio::fs::create_dir_all(&path).await?;
        Ok(Self { root: path })
    }

    /// Returns the root path of this filesystem.
    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    /// Resolves a `FsPath` to an absolute `std::path::PathBuf`.
    fn resolve(&self, path: &FsPath) -> PathBuf {
        self.root.join(path.to_std_path())
    }
}

impl Fs for NativeFs {
    type File = NativeFile;

    async fn open(&self, path: &FsPath, options: &OpenOptions) -> Result<Self::File> {
        let full_path = self.resolve(path);

        let mut std_options = tokio::fs::OpenOptions::new();
        std_options
            .read(options.read)
            .write(options.write)
            .create(options.create)
            .truncate(options.truncate)
            .append(options.append);

        let file = std_options.open(&full_path).await?;
        Ok(NativeFile::new(file, full_path))
    }

    async fn create_dir_all(&self, path: &FsPath) -> Result<()> {
        let full_path = self.resolve(path);
        tokio::fs::create_dir_all(&full_path).await?;
        Ok(())
    }

    async fn remove_file(&self, path: &FsPath) -> Result<()> {
        let full_path = self.resolve(path);
        tokio::fs::remove_file(&full_path).await?;
        Ok(())
    }

    async fn remove_dir_all(&self, path: &FsPath) -> Result<()> {
        let full_path = self.resolve(path);
        tokio::fs::remove_dir_all(&full_path).await?;
        Ok(())
    }

    async fn rename(&self, from: &FsPath, to: &FsPath) -> Result<()> {
        let from_path = self.resolve(from);
        let to_path = self.resolve(to);
        tokio::fs::rename(&from_path, &to_path).await?;
        Ok(())
    }

    async fn metadata(&self, path: &FsPath) -> Result<Metadata> {
        let full_path = self.resolve(path);
        let meta = tokio::fs::metadata(&full_path).await?;
        Ok(Metadata {
            is_file: meta.is_file(),
            is_dir: meta.is_dir(),
            size: meta.len(),
        })
    }

    async fn read_dir(&self, path: &FsPath) -> Result<Vec<DirEntry>> {
        let full_path = self.resolve(path);
        let mut entries = Vec::new();
        let mut dir = tokio::fs::read_dir(&full_path).await?;

        while let Some(entry) = dir.next_entry().await? {
            let name = entry.file_name().to_string_lossy().into_owned();
            let meta = entry.metadata().await?;
            let entry_path = path.clone().with(name.clone());

            entries.push(DirEntry {
                name,
                path: entry_path,
                metadata: Metadata {
                    is_file: meta.is_file(),
                    is_dir: meta.is_dir(),
                    size: meta.len(),
                },
            });
        }

        Ok(entries)
    }
}
