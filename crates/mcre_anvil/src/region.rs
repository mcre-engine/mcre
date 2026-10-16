use std::io::Read;
use std::path::Path;

use crate::Error;
use crate::nbt;

pub const SECTOR_BYTES: usize = 4096;
pub const HEADER_SECTORS: usize = 2;
pub const COMPRESSION_GZIP: u8 = 1;
pub const COMPRESSION_ZLIB: u8 = 2;
pub const COMPRESSION_NONE: u8 = 3;

pub struct RegionFile {
    bytes: Vec<u8>,
}

impl RegionFile {
    pub fn open(path: &Path) -> Result<Self, Error> {
        let bytes = std::fs::read(path).map_err(|err| Error::Io(err.to_string()))?;
        if bytes.len() < HEADER_SECTORS * SECTOR_BYTES {
            return Err(Error::RegionTooSmall(bytes.len()));
        }
        Ok(Self { bytes })
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
        self.location(local_x, local_z) != (0, 0)
    }

    pub fn chunk_payload(&self, local_x: u32, local_z: u32) -> Result<Option<Vec<u8>>, Error> {
        let (sectors_offset, sector_count) = self.location(local_x, local_z);
        if sectors_offset == 0 {
            return Ok(None);
        }
        let start = sectors_offset as usize * SECTOR_BYTES;
        let end = start
            .checked_add(sector_count as usize * SECTOR_BYTES)
            .ok_or(Error::Truncated)?;
        if end > self.bytes.len() {
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
        match compression {
            COMPRESSION_GZIP => {
                let mut decoder = flate2::read::GzDecoder::new(payload);
                let mut out = Vec::new();
                decoder
                    .read_to_end(&mut out)
                    .map_err(|err| Error::Decompress(err.to_string()))?;
                Ok(Some(out))
            }
            COMPRESSION_ZLIB => {
                let mut decoder = flate2::read::ZlibDecoder::new(payload);
                let mut out = Vec::new();
                decoder
                    .read_to_end(&mut out)
                    .map_err(|err| Error::Decompress(err.to_string()))?;
                Ok(Some(out))
            }
            COMPRESSION_NONE => Ok(Some(payload.to_vec())),
            other => Err(Error::UnknownCompression(other)),
        }
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
    let region = RegionFile::open(&path)?;
    let local_x = chunk_x.rem_euclid(32) as u32;
    let local_z = chunk_z.rem_euclid(32) as u32;
    let Some(payload) = region.chunk_payload(local_x, local_z)? else {
        return Ok(None);
    };
    let (_, root) = nbt::read_named(&payload)?;
    Ok(Some(root))
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
}
