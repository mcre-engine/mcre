use super::OpfsFile;
use crate::{DirEntry, Fs, FsPath, Metadata, OpenOptions, Result};

/// A filesystem backed by the Origin Private File System (OPFS).
///
/// This implementation uses the `opfs` crate which automatically selects
/// between OPFS (web/WASM) and tokio::fs (native) based on the target platform.
///
/// # Examples
///
/// ```ignore
/// use mcre_fs::{web::OpfsFs, FsPath};
///
/// let fs = OpfsFs::new().await?;
/// let data = fs.read(&FsPath::from("data/config.json")).await?;
/// ```
pub struct OpfsFs {
    root: opfs::persistent::DirectoryHandle,
}

impl OpfsFs {
    /// Creates a new `OpfsFs` using the application-specific OPFS directory.
    pub async fn new() -> Result<Self> {
        let root = opfs::persistent::app_specific_dir().await.map_err(|e| {
            crate::FsError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Failed to get OPFS root: {}", e),
            ))
        })?;
        Ok(Self { root })
    }

    /// Creates a new `OpfsFs` using a custom subdirectory within OPFS.
    pub async fn with_subdir(subdir: &str) -> Result<Self> {
        let root = opfs::persistent::app_specific_dir().await.map_err(|e| {
            crate::FsError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Failed to get OPFS root: {}", e),
            ))
        })?;

        use opfs::DirectoryHandle as _;
        let options = opfs::GetDirectoryHandleOptions { create: true };
        let dir = root
            .get_directory_handle_with_options(subdir, &options)
            .await
            .map_err(|e| {
                crate::FsError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Failed to get subdir: {}", e),
                ))
            })?;

        Ok(Self { root: dir })
    }

    /// Resolves an `FsPath` to the OPFS directory/file name at that level.
    fn resolve_name(path: &FsPath) -> Result<String> {
        path.file_name()
            .map(|s| s.to_string())
            .ok_or_else(|| crate::FsError::InvalidPath {
                reason: "Empty path".into(),
            })
    }

    async fn navigate_to_parent(&self, path: &FsPath) -> Result<opfs::persistent::DirectoryHandle> {
        use opfs::DirectoryHandle as _;

        let mut current = self.root.clone();

        if let Some(parent) = path.parent() {
            for part in parent.parts() {
                let part_name = part.as_ref();
                let options = opfs::GetDirectoryHandleOptions { create: false };
                current = current
                    .get_directory_handle_with_options(part_name, &options)
                    .await
                    .map_err(|e| {
                        crate::FsError::Io(std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            format!("Failed to navigate to {}: {}", part_name, e),
                        ))
                    })?;
            }
        }

        Ok(current)
    }

    async fn navigate_to(&self, path: &FsPath) -> Result<opfs::persistent::DirectoryHandle> {
        use opfs::DirectoryHandle as _;

        let mut current = self.root.clone();

        for part in path.parts() {
            let part_name = part.as_ref();
            let options = opfs::GetDirectoryHandleOptions { create: false };
            current = current
                .get_directory_handle_with_options(part_name, &options)
                .await
                .map_err(|e| {
                    crate::FsError::Io(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("Failed to navigate to {}: {}", part_name, e),
                    ))
                })?;
        }

        Ok(current)
    }
}

impl Fs for OpfsFs {
    type File = OpfsFile;

    async fn open(&self, path: &FsPath, options: &OpenOptions) -> Result<Self::File> {
        use opfs::DirectoryHandle as _;

        let name = Self::resolve_name(path)?;
        let dir = self.navigate_to_parent(path).await?;

        let get_options = opfs::GetFileHandleOptions {
            create: options.create || options.write,
        };

        let file_handle = dir
            .get_file_handle_with_options(&name, &get_options)
            .await
            .map_err(|e| {
                crate::FsError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Failed to get file {}: {}", name, e),
                ))
            })?;

        Ok(OpfsFile::new(file_handle))
    }

    async fn create_dir_all(&self, path: &FsPath) -> Result<()> {
        use opfs::DirectoryHandle as _;

        let mut current = self.root.clone();

        for part in path.parts() {
            let part_name = part.as_ref();
            let options = opfs::GetDirectoryHandleOptions { create: true };
            current = current
                .get_directory_handle_with_options(part_name, &options)
                .await
                .map_err(|e| {
                    crate::FsError::Io(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("Failed to create dir {}: {}", part_name, e),
                    ))
                })?;
        }

        Ok(())
    }

    async fn remove_file(&self, path: &FsPath) -> Result<()> {
        use opfs::DirectoryHandle as _;

        let name = Self::resolve_name(path)?;
        let mut dir = self.navigate_to_parent(path).await?;

        dir.remove_entry(&name).await.map_err(|e| {
            crate::FsError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Failed to remove {}: {}", name, e),
            ))
        })?;

        Ok(())
    }

    async fn remove_dir_all(&self, path: &FsPath) -> Result<()> {
        use opfs::DirectoryHandle as _;

        let name = Self::resolve_name(path)?;
        let mut dir = self.navigate_to_parent(path).await?;

        dir.remove_entry(&name).await.map_err(|e| {
            crate::FsError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Failed to remove dir {}: {}", name, e),
            ))
        })?;

        Ok(())
    }

    async fn rename(&self, from: &FsPath, to: &FsPath) -> Result<()> {
        use opfs::DirectoryHandle as _;
        use opfs::FileHandle as _;

        let from_name = Self::resolve_name(from)?;
        let to_name = Self::resolve_name(to)?;

        if let (Some(from_parent), Some(to_parent)) = (from.parent(), to.parent()) {
            if from_parent == to_parent {
                let mut dir = self.navigate_to_parent(from).await?;

                let from_handle = dir
                    .get_file_handle_with_options(
                        &from_name,
                        &opfs::GetFileHandleOptions { create: false },
                    )
                    .await
                    .map_err(|e| {
                        crate::FsError::Io(std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            format!("Failed to get source: {}", e),
                        ))
                    })?;

                let data = from_handle.read().await.map_err(|e| {
                    crate::FsError::Io(std::io::Error::other(format!(
                        "Failed to read source: {}",
                        e
                    )))
                })?;

                dir.remove_entry(&from_name).await.map_err(|e| {
                    crate::FsError::Io(std::io::Error::other(format!(
                        "Failed to remove source: {}",
                        e
                    )))
                })?;

                let mut new_handle = dir
                    .get_file_handle_with_options(
                        &to_name,
                        &opfs::GetFileHandleOptions { create: true },
                    )
                    .await
                    .map_err(|e| {
                        crate::FsError::Io(std::io::Error::other(format!(
                            "Failed to create target: {}",
                            e
                        )))
                    })?;

                let write_options = opfs::CreateWritableOptions {
                    keep_existing_data: false,
                };
                let mut writer = new_handle
                    .create_writable_with_options(&write_options)
                    .await
                    .map_err(|e| {
                        crate::FsError::Io(std::io::Error::other(format!(
                            "Failed to create writer: {}",
                            e
                        )))
                    })?;

                use opfs::WritableFileStream as _;
                writer.write_at_cursor_pos(&data).await.map_err(|e| {
                    crate::FsError::Io(std::io::Error::other(format!("Failed to write: {}", e)))
                })?;

                writer.close().await.map_err(|e| {
                    crate::FsError::Io(std::io::Error::other(format!(
                        "Failed to close writer: {}",
                        e
                    )))
                })?;
            } else {
                return Err(crate::FsError::NotSupported(
                    "Cross-directory rename not supported on OPFS".into(),
                ));
            }
        } else {
            return Err(crate::FsError::NotSupported(
                "Rename from/to root not supported".into(),
            ));
        }

        Ok(())
    }

    async fn metadata(&self, path: &FsPath) -> Result<Metadata> {
        use opfs::DirectoryHandle as _;

        let name = Self::resolve_name(path)?;
        let dir = self.navigate_to_parent(path).await?;

        let file_result = dir
            .get_file_handle_with_options(&name, &opfs::GetFileHandleOptions { create: false })
            .await;
        let dir_result = dir
            .get_directory_handle_with_options(
                &name,
                &opfs::GetDirectoryHandleOptions { create: false },
            )
            .await;

        if file_result.is_ok() {
            Ok(Metadata {
                is_file: true,
                is_dir: false,
                size: 0,
            })
        } else if dir_result.is_ok() {
            Ok(Metadata {
                is_file: false,
                is_dir: true,
                size: 0,
            })
        } else {
            Err(crate::FsError::NotFound {
                path: path.to_string(),
            })
        }
    }

    async fn read_dir(&self, path: &FsPath) -> Result<Vec<DirEntry>> {
        use futures_util::StreamExt as _;
        use opfs::DirectoryHandle as _;

        let dir = self.navigate_to(path).await?;

        let mut entries = Vec::new();
        let mut iter = dir.entries().await.map_err(|e| {
            crate::FsError::Io(std::io::Error::other(format!(
                "Failed to list entries: {}",
                e
            )))
        })?;

        while let Some(entry) = iter.next().await {
            let (name, _handle) = entry.map_err(|e| {
                crate::FsError::Io(std::io::Error::other(format!(
                    "Failed to read entry: {}",
                    e
                )))
            })?;

            let entry_path = path.clone().with(name.clone());
            // FIXME: OPFS does not distinguish files from directories in entries.
            // All entries are marked as files, which may cause issues for code
            // that relies on DirEntry::metadata.is_dir.
            entries.push(DirEntry {
                name,
                path: entry_path,
                metadata: Metadata {
                    is_file: true,
                    is_dir: false,
                    size: 0,
                },
            });
        }

        Ok(entries)
    }
}
