use crate::{FsError, OpenOptions, Result};
use embedded_io_async::{ErrorType, Read, Seek, Write};

pub struct OpfsFile {
    inner: opfs::persistent::FileHandle,
    data: Vec<u8>,
    position: usize,
    can_read: bool,
    can_write: bool,
    append: bool,
    dirty: bool,
}

impl OpfsFile {
    pub(crate) fn new(
        inner: opfs::persistent::FileHandle,
        mut data: Vec<u8>,
        options: &OpenOptions,
    ) -> Self {
        if options.truncate {
            data.clear();
        }
        let position = if options.append { data.len() } else { 0 };
        Self {
            inner,
            data,
            position,
            can_read: options.read,
            can_write: options.write || options.append,
            append: options.append,
            dirty: options.truncate,
        }
    }

    pub fn inner(&self) -> &opfs::persistent::FileHandle {
        &self.inner
    }

    pub fn into_inner(self) -> opfs::persistent::FileHandle {
        self.inner
    }

    fn permission_error(operation: &str) -> FsError {
        FsError::PermissionDenied {
            path: format!("file is not open for {operation}"),
        }
    }
}

impl ErrorType for OpfsFile {
    type Error = FsError;
}

impl Read for OpfsFile {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if !self.can_read {
            return Err(Self::permission_error("reading"));
        }
        let count = buf.len().min(self.data.len().saturating_sub(self.position));
        buf[..count].copy_from_slice(&self.data[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }

    async fn read_exact(
        &mut self,
        buf: &mut [u8],
    ) -> core::result::Result<(), embedded_io_async::ReadExactError<FsError>> {
        if !self.can_read {
            return Err(embedded_io_async::ReadExactError::Other(
                Self::permission_error("reading"),
            ));
        }
        if buf.len() > self.data.len().saturating_sub(self.position) {
            return Err(embedded_io_async::ReadExactError::UnexpectedEof);
        }
        let end = self.position + buf.len();
        buf.copy_from_slice(&self.data[self.position..end]);
        self.position = end;
        Ok(())
    }
}

impl Write for OpfsFile {
    async fn write(&mut self, buf: &[u8]) -> Result<usize> {
        if !self.can_write {
            return Err(Self::permission_error("writing"));
        }
        if self.append {
            self.position = self.data.len();
        }
        let end = self
            .position
            .checked_add(buf.len())
            .ok_or(FsError::FileTooLarge)?;
        if end > self.data.len() {
            self.data.resize(end, 0);
        }
        self.data[self.position..end].copy_from_slice(buf);
        self.position = end;
        self.dirty = true;
        Ok(buf.len())
    }

    async fn flush(&mut self) -> Result<()> {
        if !self.can_write || !self.dirty {
            return Ok(());
        }

        use opfs::FileHandle as _;
        use opfs::WritableFileStream as _;

        let options = opfs::CreateWritableOptions {
            keep_existing_data: false,
        };
        let mut writer = self
            .inner
            .create_writable_with_options(&options)
            .await
            .map_err(super::map_error)?;
        writer
            .write_at_cursor_pos(&self.data)
            .await
            .map_err(super::map_error)?;
        writer.close().await.map_err(super::map_error)?;
        self.dirty = false;
        Ok(())
    }
}

impl Seek for OpfsFile {
    async fn seek(&mut self, pos: embedded_io_async::SeekFrom) -> Result<u64> {
        let new_position = match pos {
            embedded_io_async::SeekFrom::Start(position) => i128::from(position),
            embedded_io_async::SeekFrom::Current(offset) => {
                self.position as i128 + i128::from(offset)
            }
            embedded_io_async::SeekFrom::End(offset) => {
                self.data.len() as i128 + i128::from(offset)
            }
        };
        if !(0..=usize::MAX as i128).contains(&new_position) {
            return Err(FsError::InvalidSeek);
        }
        self.position = new_position as usize;
        Ok(self.position as u64)
    }
}
