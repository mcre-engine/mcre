use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::Error;
use crate::nbt;

pub const SECTOR_BYTES: usize = 4096;
pub const HEADER_SECTORS: usize = 2;
pub const COMPRESSION_GZIP: u8 = 1;
pub const COMPRESSION_ZLIB: u8 = 2;
pub const COMPRESSION_NONE: u8 = 3;

fn decompress_payload(compression: u8, payload: &[u8]) -> Result<Vec<u8>, Error> {
    match compression {
        COMPRESSION_GZIP => crate::read_limited(
            flate2::read::GzDecoder::new(payload),
            crate::MAX_DECOMPRESSED_BYTES,
        ),
        COMPRESSION_ZLIB => crate::read_limited(
            flate2::read::ZlibDecoder::new(payload),
            crate::MAX_DECOMPRESSED_BYTES,
        ),
        COMPRESSION_NONE => {
            if payload.len() as u64 > crate::MAX_DECOMPRESSED_BYTES {
                return Err(Error::DecompressedTooLarge(crate::MAX_DECOMPRESSED_BYTES));
            }
            Ok(payload.to_vec())
        }
        other => Err(Error::UnknownCompression(other)),
    }
}

pub struct RegionFile {
    bytes: Vec<u8>,
}

/// A header entry is usable only when it points past the two header sectors
/// and claims at least one sector. Vanilla rejects both cases; treating them
/// as absent keeps a corrupt header from panicking or reading header bytes as
/// chunk data.
fn valid_sector_location((sectors_offset, sector_count): (u32, u32)) -> bool {
    sectors_offset >= HEADER_SECTORS as u32 && sector_count != 0
}

impl RegionFile {
    pub fn open(path: &Path) -> Result<Self, Error> {
        let bytes = std::fs::read(path).map_err(|err| Error::Io(err.to_string()))?;
        Self::from_bytes(bytes)
    }

    /// Parses a region file already held in memory. The browser save loader
    /// reads `.mca` files through OPFS and uses this instead of
    /// [`RegionFile::open`].
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, Error> {
        if bytes.len() < HEADER_SECTORS * SECTOR_BYTES {
            return Err(Error::RegionTooSmall(bytes.len()));
        }
        Ok(Self { bytes })
    }

    /// Reads the NBT root for a chunk. The file must be the region that
    /// contains `chunk_x`/`chunk_z`; callers derive the path from those
    /// coordinates (see [`chunk_nbt`]).
    pub fn chunk_nbt(&self, chunk_x: i32, chunk_z: i32) -> Result<Option<nbt::Tag>, Error> {
        let local_x = chunk_x.rem_euclid(32) as u32;
        let local_z = chunk_z.rem_euclid(32) as u32;
        let Some(payload) = self.chunk_payload(local_x, local_z)? else {
            return Ok(None);
        };
        let (_, root) = nbt::read_named(&payload)?;
        Ok(Some(root))
    }

    fn location(&self, local_x: u32, local_z: u32) -> (u32, u32) {
        let index = ((local_x & 31) + (local_z & 31) * 32) as usize * 4;
        let entry = u32::from_be_bytes([
            self.bytes[index],
            self.bytes[index + 1],
            self.bytes[index + 2],
            self.bytes[index + 3],
        ]);
        (entry >> 8, entry & 255)
    }

    pub fn has_chunk(&self, local_x: u32, local_z: u32) -> bool {
        valid_sector_location(self.location(local_x, local_z))
    }

    pub fn chunk_payload(&self, local_x: u32, local_z: u32) -> Result<Option<Vec<u8>>, Error> {
        let (sectors_offset, sector_count) = self.location(local_x, local_z);
        if !valid_sector_location((sectors_offset, sector_count)) {
            return Ok(None);
        }
        let start = sectors_offset as usize * SECTOR_BYTES;
        let end = start
            .checked_add(sector_count as usize * SECTOR_BYTES)
            .ok_or(Error::Truncated)?;
        if end > self.bytes.len() {
            return Err(Error::Truncated);
        }
        if start + 5 > end {
            return Err(Error::Truncated);
        }
        let length = u32::from_be_bytes([
            self.bytes[start],
            self.bytes[start + 1],
            self.bytes[start + 2],
            self.bytes[start + 3],
        ]) as usize;
        if length == 0 || start + 5 + length - 1 > end {
            return Err(Error::Truncated);
        }
        let compression = self.bytes[start + 4];
        let payload = &self.bytes[start + 5..start + 5 + length - 1];
        decompress_payload(compression, payload).map(Some)
    }

    pub fn chunk_count(&self) -> usize {
        let mut count = 0;
        for z in 0..32 {
            for x in 0..32 {
                if self.has_chunk(x, z) {
                    count += 1;
                }
            }
        }
        count
    }
}

pub fn chunk_nbt(region_dir: &Path, chunk_x: i32, chunk_z: i32) -> Result<Option<nbt::Tag>, Error> {
    let region_x = chunk_x.div_euclid(32);
    let region_z = chunk_z.div_euclid(32);
    let path = region_dir.join(format!("r.{region_x}.{region_z}.mca"));
    if !path.exists() {
        return Ok(None);
    }
    RegionFile::open(&path)?.chunk_nbt(chunk_x, chunk_z)
}

pub fn region_file_path(region_dir: &Path, chunk_x: i32, chunk_z: i32) -> PathBuf {
    let region_x = chunk_x.div_euclid(32);
    let region_z = chunk_z.div_euclid(32);
    region_dir.join(format!("r.{region_x}.{region_z}.mca"))
}

fn read_header_entry(
    file: &mut std::fs::File,
    local_x: u32,
    local_z: u32,
) -> Result<(u32, u32), Error> {
    let mut header = [0u8; HEADER_SECTORS * SECTOR_BYTES];
    file.read_exact(&mut header)
        .map_err(|err| match err.kind() {
            std::io::ErrorKind::UnexpectedEof => Error::Truncated,
            _ => Error::Io(err.to_string()),
        })?;
    let index = ((local_x & 31) + (local_z & 31) * 32) as usize * 4;
    let entry = u32::from_be_bytes([
        header[index],
        header[index + 1],
        header[index + 2],
        header[index + 3],
    ]);
    Ok((entry >> 8, entry & 255))
}

fn read_payload_from_file(
    path: &Path,
    local_x: u32,
    local_z: u32,
) -> Result<Option<Vec<u8>>, Error> {
    let mut file = std::fs::File::open(path).map_err(|err| Error::Io(err.to_string()))?;
    let (sectors_offset, sector_count) = read_header_entry(&mut file, local_x, local_z)?;
    if !valid_sector_location((sectors_offset, sector_count)) {
        return Ok(None);
    }
    let start = sectors_offset as u64 * SECTOR_BYTES as u64;
    let end = start + sector_count as u64 * SECTOR_BYTES as u64;
    let file_len = file
        .metadata()
        .map_err(|err| Error::Io(err.to_string()))?
        .len();
    if end > file_len {
        return Err(Error::Truncated);
    }
    if start + 5 > end {
        return Err(Error::Truncated);
    }
    file.seek(SeekFrom::Start(start))
        .map_err(|err| Error::Io(err.to_string()))?;
    let mut length_buf = [0u8; 4];
    file.read_exact(&mut length_buf)
        .map_err(|err| Error::Io(err.to_string()))?;
    let length = u32::from_be_bytes(length_buf) as u64;
    if length == 0 || start + 5 + length - 1 > end {
        return Err(Error::Truncated);
    }
    let mut compression = [0u8; 1];
    file.read_exact(&mut compression)
        .map_err(|err| Error::Io(err.to_string()))?;
    let mut payload = vec![0u8; (length - 1) as usize];
    file.read_exact(&mut payload)
        .map_err(|err| Error::Io(err.to_string()))?;
    decompress_payload(compression[0], &payload).map(Some)
}

/// Read the chunk's worldgen status (e.g. `minecraft:full`) without parsing
/// the full NBT tree. Returns `Ok(None)` when the chunk is not on disk yet.
pub fn chunk_status(
    region_dir: &Path,
    chunk_x: i32,
    chunk_z: i32,
) -> Result<Option<String>, Error> {
    let path = region_file_path(region_dir, chunk_x, chunk_z);
    if !path.exists() {
        return Ok(None);
    }
    let local_x = chunk_x.rem_euclid(32) as u32;
    let local_z = chunk_z.rem_euclid(32) as u32;
    let Some(payload) = read_payload_from_file(&path, local_x, local_z)? else {
        return Ok(None);
    };
    if let Some(status) = nbt::find_root_string(&payload, "status")? {
        return Ok(Some(status));
    }
    nbt::find_root_string(&payload, "Status")
}

/// Cheap presence check: reads only the 8 KiB region header.
pub fn chunk_present(region_dir: &Path, chunk_x: i32, chunk_z: i32) -> Result<bool, Error> {
    let path = region_file_path(region_dir, chunk_x, chunk_z);
    if !path.exists() {
        return Ok(false);
    }
    let local_x = chunk_x.rem_euclid(32) as u32;
    let local_z = chunk_z.rem_euclid(32) as u32;
    let mut file = std::fs::File::open(&path).map_err(|err| Error::Io(err.to_string()))?;
    let (sectors_offset, sector_count) = read_header_entry(&mut file, local_x, local_z)?;
    Ok(valid_sector_location((sectors_offset, sector_count)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use std::io::Write;

    fn test_region(chunk_nbt_payload: &[u8]) -> RegionFile {
        let mut compressed = Vec::new();
        ZlibEncoder::new(&mut compressed, Compression::default())
            .write_all(chunk_nbt_payload)
            .unwrap();
        let body_len = (compressed.len() + 1) as u32;
        let total = 5 + compressed.len();
        let sectors = total.div_ceil(SECTOR_BYTES);
        let mut bytes = vec![0u8; HEADER_SECTORS * SECTOR_BYTES + sectors * SECTOR_BYTES];
        let entry = (2u32 << 8) | sectors as u32;
        bytes[0..4].copy_from_slice(&entry.to_be_bytes());
        bytes[8192..8196].copy_from_slice(&body_len.to_be_bytes());
        bytes[8196] = COMPRESSION_ZLIB;
        bytes[8197..8197 + compressed.len()].copy_from_slice(&compressed);
        RegionFile { bytes }
    }

    fn sample_payload() -> Vec<u8> {
        let mut out = vec![10, 0, 0];
        out.push(3);
        out.extend_from_slice(&[0, 4]);
        out.extend_from_slice(b"xPos");
        out.extend_from_slice(&(-3i32).to_be_bytes());
        out.push(0);
        out
    }

    #[test]
    fn round_trips_zlib_chunk() {
        let region = test_region(&sample_payload());
        assert!(region.has_chunk(0, 0));
        assert!(!region.has_chunk(1, 0));
        assert_eq!(region.chunk_count(), 1);
        let payload = region.chunk_payload(0, 0).unwrap().unwrap();
        let (_, root) = nbt::read_named(&payload).unwrap();
        assert_eq!(root.get("xPos"), Some(&nbt::Tag::Int(-3)));
        assert!(region.chunk_payload(1, 0).unwrap().is_none());
    }

    #[test]
    fn reads_chunk_from_bytes() {
        let region = test_region(&sample_payload());
        let parsed = RegionFile::from_bytes(region.bytes.clone()).unwrap();
        let root = parsed.chunk_nbt(0, 0).unwrap().unwrap();
        assert_eq!(root.get("xPos"), Some(&nbt::Tag::Int(-3)));
        assert!(parsed.chunk_nbt(1, 0).unwrap().is_none());
    }

    #[test]
    fn from_bytes_rejects_short_files() {
        assert!(matches!(
            RegionFile::from_bytes(vec![0; 100]),
            Err(Error::RegionTooSmall(_))
        ));
    }

    #[test]
    fn rejects_short_files() {
        let dir = std::env::temp_dir();
        let path = dir.join("mcre_anvil_short_test.mca");
        std::fs::write(&path, vec![0u8; 100]).unwrap();
        assert!(matches!(
            RegionFile::open(&path),
            Err(Error::RegionTooSmall(_))
        ));
        std::fs::remove_file(&path).ok();
    }

    fn region_with_header(entry: u32) -> RegionFile {
        let mut bytes = vec![0u8; HEADER_SECTORS * SECTOR_BYTES + SECTOR_BYTES];
        bytes[0..4].copy_from_slice(&entry.to_be_bytes());
        RegionFile::from_bytes(bytes).unwrap()
    }

    #[test]
    fn zero_sector_count_is_a_missing_chunk() {
        let region = region_with_header(2u32 << 8);
        assert!(!region.has_chunk(0, 0));
        assert_eq!(region.chunk_count(), 0);
        assert!(region.chunk_payload(0, 0).unwrap().is_none());
    }

    #[test]
    fn header_sector_offset_is_rejected() {
        let region = region_with_header((1u32 << 8) | 1);
        assert!(!region.has_chunk(0, 0));
        assert!(region.chunk_payload(0, 0).unwrap().is_none());
    }

    #[test]
    fn payload_length_past_sector_is_truncated() {
        let region = region_with_header((2u32 << 8) | 1);
        let mut bytes = region.bytes;
        bytes[8192..8196].copy_from_slice(&(SECTOR_BYTES as u32).to_be_bytes());
        let region = RegionFile::from_bytes(bytes).unwrap();
        assert!(matches!(region.chunk_payload(0, 0), Err(Error::Truncated)));
    }

    fn status_payload(status: &str) -> Vec<u8> {
        let mut out = vec![10, 0, 0];
        out.push(3);
        out.extend_from_slice(&[0, 4]);
        out.extend_from_slice(b"xPos");
        out.extend_from_slice(&0i32.to_be_bytes());
        out.push(8);
        out.extend_from_slice(&[0, 6]);
        out.extend_from_slice(b"status");
        out.extend_from_slice(&(status.len() as u16).to_be_bytes());
        out.extend_from_slice(status.as_bytes());
        out.push(0);
        out
    }

    #[test]
    fn reads_status_and_presence_from_file() {
        let dir = std::env::temp_dir().join(format!("mcre_anvil_status_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("r.0.0.mca");
        let region = test_region(&status_payload("minecraft:full"));
        std::fs::write(&path, &region.bytes).unwrap();

        assert!(chunk_present(&dir, 0, 0).unwrap());
        assert!(!chunk_present(&dir, 1, 0).unwrap());
        assert!(!chunk_present(&dir, 32, 0).unwrap());
        assert_eq!(
            chunk_status(&dir, 0, 0).unwrap(),
            Some("minecraft:full".to_string())
        );
        assert_eq!(chunk_status(&dir, 1, 0).unwrap(), None);
        assert_eq!(chunk_status(&dir, 32, 0).unwrap(), None);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn decompression_limit_rejects_oversized_output() {
        let data = vec![0u8; 1024];
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&data).unwrap();
        let compressed = encoder.finish().unwrap();

        assert_eq!(
            decompress_payload(COMPRESSION_ZLIB, &compressed).unwrap(),
            data
        );
        assert!(matches!(
            crate::read_limited(flate2::read::ZlibDecoder::new(&compressed[..]), 1023),
            Err(Error::DecompressedTooLarge(1023))
        ));
    }
}
