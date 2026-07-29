use crate::{FsError, Result};
use embedded_io_async::{ErrorType, Read, Seek, Write};

pub struct OpfsFile {
    inner: opfs::persistent::FileHandle,
    read_cache: Option<Vec<u8>>,
    read_pos: usize,
    write_pos: usize,
}

impl OpfsFile {
    pub(crate) fn new(inner: opfs::persistent::FileHandle) -> Self {
        Self {
            inner,
            read_cache: None,
            read_pos: 0,
            write_pos: 0,
        }
    }

    async fn ensure_loaded(&mut self) -> Result<()> {
        if self.read_cache.is_none() {
            use opfs::FileHandle as _;
            let data = self.inner.read().await.map_err(|e| {
                FsError::Io(std::io::Error::other(format!("OPFS read error: {}", e)))
            })?;
            self.write_pos = data.len();
            self.read_cache = Some(data);
        }
        Ok(())
    }

    pub fn inner(&self) -> &opfs::persistent::FileHandle {
        &self.inner
    }

    pub fn into_inner(self) -> opfs::persistent::FileHandle {
        self.inner
    }
}

impl ErrorType for OpfsFile {
    type Error = FsError;
}

impl Read for OpfsFile {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.ensure_loaded().await?;

        let cached = self.read_cache.as_ref().unwrap();
        let remaining = &cached[self.read_pos..];
        let len = remaining.len().min(buf.len());
        buf[..len].copy_from_slice(&remaining[..len]);
        self.read_pos += len;
        Ok(len)
    }

    async fn read_exact(
        &mut self,
        buf: &mut [u8],
    ) -> core::result::Result<(), embedded_io_async::ReadExactError<FsError>> {
        self.ensure_loaded()
            .await
            .map_err(embedded_io_async::ReadExactError::Other)?;

        let cached = self.read_cache.as_ref().unwrap();
        let remaining = &cached[self.read_pos..];
        if remaining.len() < buf.len() {
            return Err(embedded_io_async::ReadExactError::UnexpectedEof);
        }
        buf.copy_from_slice(&remaining[..buf.len()]);
        self.read_pos += buf.len();
        Ok(())
    }
}

impl Write for OpfsFile {
    async fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.ensure_loaded().await?;
        self.read_cache = None;
        self.read_pos = 0;

        use opfs::FileHandle as _;
        use opfs::WritableFileStream as _;

        let options = opfs::CreateWritableOptions {
            keep_existing_data: true,
        };
        let mut writer = self
            .inner
            .create_writable_with_options(&options)
            .await
            .map_err(|e| {
                FsError::Io(std::io::Error::other(format!(
                    "OPFS create writer error: {}",
                    e
                )))
            })?;

        writer
            .seek(self.write_pos)
            .await
            .map_err(|e| FsError::Io(std::io::Error::other(format!("OPFS seek error: {}", e))))?;

        writer
            .write_at_cursor_pos(buf)
            .await
            .map_err(|e| FsError::Io(std::io::Error::other(format!("OPFS write error: {}", e))))?;

        writer.close().await.map_err(|e| {
            FsError::Io(std::io::Error::other(format!(
                "OPFS close writer error: {}",
                e
            )))
        })?;

        self.write_pos += buf.len();
        Ok(buf.len())
    }

    async fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

impl Seek for OpfsFile {
    async fn seek(&mut self, _pos: embedded_io_async::SeekFrom) -> Result<u64> {
        Err(FsError::NotSupported(
            "Seek not supported on OPFS files".into(),
        ))
    }
}
