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
        let root = opfs::persistent::app_specific_dir()
            .await
            .map_err(super::map_error)?;
        Ok(Self { root })
    }

    /// Creates a new `OpfsFs` using a custom subdirectory within OPFS.
    pub async fn with_subdir(subdir: &str) -> Result<Self> {
        let root = opfs::persistent::app_specific_dir()
            .await
            .map_err(super::map_error)?;

        use opfs::DirectoryHandle as _;
        let options = opfs::GetDirectoryHandleOptions { create: true };
        let dir = root
            .get_directory_handle_with_options(subdir, &options)
            .await
            .map_err(super::map_error)?;

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
                    .map_err(super::map_error)?;
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
                .map_err(super::map_error)?;
        }

        Ok(current)
    }
}

impl Fs for OpfsFs {
    type File = OpfsFile;

    async fn open(&self, path: &FsPath, options: &OpenOptions) -> Result<Self::File> {
        use opfs::DirectoryHandle as _;
        use opfs::FileHandle as _;

        if !options.read && !options.write && !options.append {
            return Err(crate::FsError::InvalidPath {
                reason: "Open must request read or write access".into(),
            });
        }
        if options.truncate && !options.write {
            return Err(crate::FsError::InvalidPath {
                reason: "Truncate requires write access".into(),
            });
        }

        let name = Self::resolve_name(path)?;
        let dir = self.navigate_to_parent(path).await?;

        let get_options = opfs::GetFileHandleOptions {
            create: options.create,
        };

        let file_handle = dir
            .get_file_handle_with_options(&name, &get_options)
            .await
            .map_err(super::map_error)?;

        let data = file_handle.read().await.map_err(super::map_error)?;
        Ok(OpfsFile::new(file_handle, data, options))
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
                .map_err(super::map_error)?;
        }

        Ok(())
    }

    async fn remove_file(&self, path: &FsPath) -> Result<()> {
        use opfs::DirectoryHandle as _;

        let name = Self::resolve_name(path)?;
        let mut dir = self.navigate_to_parent(path).await?;

        dir.remove_entry(&name).await.map_err(super::map_error)?;

        Ok(())
    }

    async fn remove_dir_all(&self, path: &FsPath) -> Result<()> {
        use opfs::DirectoryHandle as _;

        let name = Self::resolve_name(path)?;
        let mut dir = self.navigate_to_parent(path).await?;

        dir.remove_entry_with_options(&name, &opfs::FileSystemRemoveOptions { recursive: true })
            .await
            .map_err(super::map_error)?;

        Ok(())
    }

    async fn rename(&self, from: &FsPath, to: &FsPath) -> Result<()> {
        use opfs::DirectoryHandle as _;
        use opfs::FileHandle as _;

        let from_name = Self::resolve_name(from)?;
        let to_name = Self::resolve_name(to)?;
        if from == to {
            return Ok(());
        }

        let mut from_parent = self.navigate_to_parent(from).await?;
        let to_parent = self.navigate_to_parent(to).await?;
        let source_file = from_parent
            .get_file_handle_with_options(&from_name, &opfs::GetFileHandleOptions { create: false })
            .await;
        if let Ok(file) = source_file {
            let data = file.read().await.map_err(super::map_error)?;
            write_file(&to_parent, &to_name, &data).await?;
        } else {
            let source_dir = from_parent
                .get_directory_handle_with_options(
                    &from_name,
                    &opfs::GetDirectoryHandleOptions { create: false },
                )
                .await
                .map_err(super::map_error)?;
            if to.parts().starts_with(from.parts()) {
                return Err(crate::FsError::InvalidPath {
                    reason: "Cannot move a directory into itself".into(),
                });
            }
            let target_dir = to_parent
                .get_directory_handle_with_options(
                    &to_name,
                    &opfs::GetDirectoryHandleOptions { create: true },
                )
                .await
                .map_err(super::map_error)?;
            copy_directory(source_dir, target_dir).await?;
        }

        from_parent
            .remove_entry_with_options(
                &from_name,
                &opfs::FileSystemRemoveOptions { recursive: true },
            )
            .await
            .map_err(super::map_error)
    }

    async fn metadata(&self, path: &FsPath) -> Result<Metadata> {
        use opfs::DirectoryHandle as _;
        use opfs::FileHandle as _;

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

        match file_result {
            Ok(file) => {
                let size = file.size().await.map_err(super::map_error)?;
                Ok(Metadata {
                    is_file: true,
                    is_dir: false,
                    size: size as u64,
                })
            }
            Err(file_error) => match dir_result {
                Ok(_) => Ok(Metadata {
                    is_file: false,
                    is_dir: true,
                    size: 0,
                }),
                Err(_) => Err(super::map_error(file_error)),
            },
        }
    }

    async fn read_dir(&self, path: &FsPath) -> Result<Vec<DirEntry>> {
        use futures_util::StreamExt as _;
        use opfs::DirectoryEntry as OpfsDirectoryEntry;
        use opfs::DirectoryHandle as _;
        use opfs::FileHandle as _;

        let dir = self.navigate_to(path).await?;

        let mut entries = Vec::new();
        let mut iter = dir.entries().await.map_err(super::map_error)?;

        while let Some(entry) = iter.next().await {
            let (name, handle) = entry.map_err(super::map_error)?;

            let entry_path = path.clone().with(name.clone());
            let metadata = match handle {
                OpfsDirectoryEntry::File(file) => Metadata {
                    is_file: true,
                    is_dir: false,
                    size: file.size().await.map_err(super::map_error)? as u64,
                },
                OpfsDirectoryEntry::Directory(_) => Metadata {
                    is_file: false,
                    is_dir: true,
                    size: 0,
                },
            };
            entries.push(DirEntry {
                name,
                path: entry_path,
                metadata,
            });
        }

        Ok(entries)
    }
}

async fn write_file(
    parent: &opfs::persistent::DirectoryHandle,
    name: &str,
    data: &[u8],
) -> Result<()> {
    use opfs::DirectoryHandle as _;
    use opfs::FileHandle as _;
    use opfs::WritableFileStream as _;

    let mut handle = parent
        .get_file_handle_with_options(name, &opfs::GetFileHandleOptions { create: true })
        .await
        .map_err(super::map_error)?;
    let mut writer = handle
        .create_writable_with_options(&opfs::CreateWritableOptions {
            keep_existing_data: false,
        })
        .await
        .map_err(super::map_error)?;
    writer
        .write_at_cursor_pos(data)
        .await
        .map_err(super::map_error)?;
    writer.close().await.map_err(super::map_error)
}

async fn copy_directory(
    source: opfs::persistent::DirectoryHandle,
    target: opfs::persistent::DirectoryHandle,
) -> Result<()> {
    use futures_util::StreamExt as _;
    use opfs::DirectoryEntry as OpfsDirectoryEntry;
    use opfs::DirectoryHandle as _;
    use opfs::FileHandle as _;

    let mut pending = vec![(source, target)];
    while let Some((source_dir, target_dir)) = pending.pop() {
        let mut entries = source_dir.entries().await.map_err(super::map_error)?;
        while let Some(entry) = entries.next().await {
            let (name, entry) = entry.map_err(super::map_error)?;
            match entry {
                OpfsDirectoryEntry::File(file) => {
                    let data = file.read().await.map_err(super::map_error)?;
                    write_file(&target_dir, &name, &data).await?;
                }
                OpfsDirectoryEntry::Directory(source_child) => {
                    let target_child = target_dir
                        .get_directory_handle_with_options(
                            &name,
                            &opfs::GetDirectoryHandleOptions { create: true },
                        )
                        .await
                        .map_err(super::map_error)?;
                    pending.push((source_child, target_child));
                }
            }
        }
    }
    Ok(())
}
