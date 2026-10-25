pub mod block_entity;
pub mod chunk;
pub mod nbt;
pub mod region;
pub mod resolve;

pub use block_entity::{
    BlockEntityPayload, InventoryItem, SignData, SignText, decode_block_entity_payload,
};
pub use chunk::{
    ImportRect, ImportStats, import_area, import_chunk, load_chunk, load_chunk_from_nbt,
    load_entities_from_nbt,
};
pub use region::{RegionFile, chunk_nbt, chunk_present, chunk_status, region_file_path};
pub use resolve::{BiomeResolver, BlockResolver};

use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(String),
    #[error("decompression failed: {0}")]
    Decompress(String),
    #[error("truncated input")]
    Truncated,
    #[error("invalid UTF-8 in NBT string")]
    InvalidUtf8,
    #[error("negative NBT length")]
    NegativeLength,
    #[error("unknown NBT tag: {0}")]
    UnknownTag(u8),
    #[error("expected root compound, found tag {0}")]
    ExpectedRootCompound(u8),
    #[error("compressed input passed to NBT parser; decompress first")]
    CompressedInput,
    #[error("region file too small: {0} bytes")]
    RegionTooSmall(usize),
    #[error("unknown region compression: {0}")]
    UnknownCompression(u8),
    #[error("missing field: {0}")]
    MissingField(&'static str),
    #[error("bit storage too short")]
    BitStorageTooShort,
    #[error("bit storage length mismatch: expected {expected} longs, found {found}")]
    BitStorageLength { expected: usize, found: usize },
    #[error("palette index out of range: {0}")]
    PaletteIndex(usize),
    #[error("chunk mismatch: expected {expected:?}, found {found:?}")]
    ChunkMismatch {
        expected: (i32, i32),
        found: (i32, i32),
    },
}

pub struct SavePaths {
    pub save_dir: PathBuf,
}

impl SavePaths {
    pub fn new(save_dir: &Path) -> Self {
        Self {
            save_dir: save_dir.to_path_buf(),
        }
    }

    pub fn region_dir(&self, dimension: &str) -> PathBuf {
        match dimension {
            "minecraft:overworld" => self.save_dir.join("dimensions/minecraft/overworld/region"),
            "minecraft:the_nether" => self.save_dir.join("dimensions/minecraft/the_nether/region"),
            "minecraft:the_end" => self.save_dir.join("dimensions/minecraft/the_end/region"),
            _ => self.save_dir.join("region"),
        }
    }

    pub fn legacy_region_dir(&self) -> PathBuf {
        self.save_dir.join("region")
    }

    pub fn resolve_region_dir(&self, dimension: &str) -> PathBuf {
        let modern = self.region_dir(dimension);
        if modern.is_dir() {
            return modern;
        }
        self.legacy_region_dir()
    }

    pub fn resolve_entity_dir(&self, dimension: &str) -> PathBuf {
        let modern = self
            .resolve_region_dir(dimension)
            .parent()
            .unwrap_or(&self.save_dir)
            .join("entities");
        if modern.is_dir() {
            modern
        } else {
            self.save_dir.join("entities")
        }
    }
}

#[derive(Debug, Clone)]
pub struct Spawn {
    pub pos: (i32, i32, i32),
    pub dimension: String,
    pub yaw: f32,
    pub pitch: f32,
}

fn gunzip(bytes: &[u8]) -> Result<Vec<u8>, Error> {
    if bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
        let mut decoder = flate2::read::GzDecoder::new(bytes);
        let mut out = Vec::new();
        decoder
            .read_to_end(&mut out)
            .map_err(|err| Error::Decompress(err.to_string()))?;
        Ok(out)
    } else {
        Ok(bytes.to_vec())
    }
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(bytes).expect("gzip write");
    encoder.finish().expect("gzip finish")
}

/// Read a gzip-compressed NBT file (e.g. `level.dat`, `world/data/minecraft/world_clocks.dat`)
/// as its root compound. Plain NBT without a gzip header is accepted too.
pub fn read_gzip_nbt(path: &Path) -> Result<nbt::Tag, Error> {
    let bytes = std::fs::read(path).map_err(|err| Error::Io(err.to_string()))?;
    read_gzip_nbt_bytes(&bytes)
}

/// Parse gzip-compressed (or plain) NBT bytes. The browser save loader uses
/// this after reading `level.dat` through OPFS.
pub fn read_gzip_nbt_bytes(bytes: &[u8]) -> Result<nbt::Tag, Error> {
    let raw = gunzip(bytes)?;
    let (_, root) = nbt::read_named(&raw)?;
    Ok(root)
}

/// Write a root compound as a gzip-compressed NBT file, creating parent
/// directories as needed.
pub fn write_gzip_nbt(path: &Path, root: &nbt::Tag) -> Result<(), Error> {
    let raw = nbt::write_named("", root);
    let bytes = gzip(&raw);
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|err| Error::Io(err.to_string()))?;
    }
    std::fs::write(path, bytes).map_err(|err| Error::Io(err.to_string()))
}

/// Read `level.dat` as the root compound (usually `{Data: {...}}`).
pub fn read_level_dat(save_dir: &Path) -> Result<nbt::Tag, Error> {
    load_level_data(save_dir)
}

/// Write `level.dat` from a root compound; the inverse of [`read_level_dat`].
pub fn write_level_dat(save_dir: &Path, root: &nbt::Tag) -> Result<(), Error> {
    write_gzip_nbt(&save_dir.join("level.dat"), root)
}

fn load_level_data(save_dir: &Path) -> Result<nbt::Tag, Error> {
    let bytes =
        std::fs::read(save_dir.join("level.dat")).map_err(|err| Error::Io(err.to_string()))?;
    read_gzip_nbt_bytes(&bytes)
}

/// Read the world seed.
///
/// Since 26.x the seed lives in `data/minecraft/world_gen_settings.dat`;
/// older saves kept it in `level.dat`.
pub fn read_seed(save_dir: &Path) -> Result<i64, Error> {
    let settings_path = save_dir
        .join("data")
        .join("minecraft")
        .join("world_gen_settings.dat");
    if let Ok(bytes) = std::fs::read(&settings_path) {
        let raw = gunzip(&bytes)?;
        if let Ok((_, root)) = nbt::read_named(&raw)
            && let Some(nbt::Tag::Long(seed)) = root.get("data").and_then(|data| data.get("seed"))
        {
            return Ok(*seed);
        }
    }

    let root = load_level_data(save_dir)?;
    let data = root.get("Data").unwrap_or(&root);
    if let Some(nbt::Tag::Long(seed)) = data
        .get("WorldGenSettings")
        .and_then(|settings| settings.get("seed"))
    {
        return Ok(*seed);
    }
    if let Some(nbt::Tag::Long(seed)) = data.get("RandomSeed") {
        return Ok(*seed);
    }
    Err(Error::MissingField("seed"))
}

pub fn read_spawn(save_dir: &Path) -> Result<Spawn, Error> {
    read_spawn_from_root(&load_level_data(save_dir)?)
}

/// Extract the spawn from an already-parsed `level.dat` root. The browser
/// save loader uses this after reading and decompressing the file itself.
pub fn read_spawn_from_root(root: &nbt::Tag) -> Result<Spawn, Error> {
    let data = root.get("Data").unwrap_or(root);
    let spawn = data.get("spawn").ok_or(Error::MissingField("spawn"))?;
    let pos = spawn.get("pos").and_then(|tag| match tag {
        nbt::Tag::IntArray(values) => {
            (values.len() == 3).then(|| (values[0], values[1], values[2]))
        }
        _ => None,
    });
    let legacy = (
        data.get("SpawnX").and_then(nbt::Tag::as_i32),
        data.get("SpawnY").and_then(nbt::Tag::as_i32),
        data.get("SpawnZ").and_then(nbt::Tag::as_i32),
    );
    let pos = pos.or(match legacy {
        (Some(x), Some(y), Some(z)) => Some((x, y, z)),
        _ => None,
    });
    let Some(pos) = pos else {
        return Err(Error::MissingField("spawn.pos"));
    };
    let dimension = spawn
        .get("dimension")
        .or_else(|| data.get("Dimension"))
        .and_then(nbt::Tag::as_str)
        .unwrap_or("minecraft:overworld")
        .to_string();
    let yaw = spawn
        .get("yaw")
        .and_then(|tag| match tag {
            nbt::Tag::Float(value) => Some(*value),
            nbt::Tag::Double(value) => Some(*value as f32),
            nbt::Tag::Int(value) => Some(*value as f32),
            _ => None,
        })
        .unwrap_or(0.0);
    let pitch = spawn
        .get("pitch")
        .and_then(|tag| match tag {
            nbt::Tag::Float(value) => Some(*value),
            nbt::Tag::Double(value) => Some(*value as f32),
            nbt::Tag::Int(value) => Some(*value as f32),
            _ => None,
        })
        .unwrap_or(0.0);
    Ok(Spawn {
        pos,
        dimension,
        yaw,
        pitch,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_seed_from_world_gen_settings_file() {
        let dir = std::env::temp_dir().join(format!("mcre_anvil_seed_wgs_{}", std::process::id()));
        let path = dir
            .join("data")
            .join("minecraft")
            .join("world_gen_settings.dat");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();

        let mut nbt = vec![nbt::COMPOUND, 0, 0];
        nbt.extend_from_slice(&[nbt::COMPOUND, 0, 4]);
        nbt.extend_from_slice(b"data");
        nbt.extend_from_slice(&[nbt::LONG, 0, 4]);
        nbt.extend_from_slice(b"seed");
        nbt.extend_from_slice(&(4242i64).to_be_bytes());
        nbt.push(0);
        nbt.push(0);

        let mut gzip = Vec::new();
        {
            use std::io::Write;
            let mut encoder =
                flate2::write::GzEncoder::new(&mut gzip, flate2::Compression::default());
            encoder.write_all(&nbt).unwrap();
            encoder.finish().unwrap();
        }
        std::fs::write(&path, &gzip).unwrap();

        assert_eq!(read_seed(&dir).unwrap(), 4242);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reads_seed_from_level_data() {
        let dir = std::env::temp_dir().join(format!("mcre_anvil_seed_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let mut bytes = vec![nbt::COMPOUND, 0, 0];
        bytes.extend_from_slice(&[nbt::COMPOUND, 0, 4]);
        bytes.extend_from_slice(b"Data");
        bytes.extend_from_slice(&[nbt::COMPOUND, 0, 16]);
        bytes.extend_from_slice(b"WorldGenSettings");
        bytes.extend_from_slice(&[nbt::LONG, 0, 4]);
        bytes.extend_from_slice(b"seed");
        bytes.extend_from_slice(&(-1234i64).to_be_bytes());
        bytes.push(0);
        bytes.push(0);
        bytes.push(0);
        std::fs::write(dir.join("level.dat"), &bytes).unwrap();

        assert_eq!(read_seed(&dir).unwrap(), -1234);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reads_gzip_nbt_bytes() {
        let nbt_bytes = vec![nbt::COMPOUND, 0, 0, 0];
        let mut gzip = Vec::new();
        {
            use std::io::Write;
            let mut encoder =
                flate2::write::GzEncoder::new(&mut gzip, flate2::Compression::default());
            encoder.write_all(&nbt_bytes).unwrap();
            encoder.finish().unwrap();
        }
        let root = read_gzip_nbt_bytes(&gzip).unwrap();
        assert!(matches!(root, nbt::Tag::Compound(_)));
    }

    #[test]
    fn reads_spawn_from_root_tag() {
        let root = nbt::Tag::Compound(vec![(
            "Data".into(),
            nbt::Tag::Compound(vec![(
                "spawn".into(),
                nbt::Tag::Compound(vec![
                    ("pos".into(), nbt::Tag::IntArray(vec![8, 70, -3])),
                    (
                        "dimension".into(),
                        nbt::Tag::String("minecraft:overworld".into()),
                    ),
                    ("yaw".into(), nbt::Tag::Float(90.0)),
                    ("pitch".into(), nbt::Tag::Float(-10.0)),
                ]),
            )]),
        )]);
        let spawn = read_spawn_from_root(&root).unwrap();
        assert_eq!(spawn.pos, (8, 70, -3));
        assert_eq!(spawn.dimension, "minecraft:overworld");
        assert_eq!(spawn.yaw, 90.0);
        assert_eq!(spawn.pitch, -10.0);
    }

    #[test]
    fn reads_legacy_seed_from_level_data() {
        let dir =
            std::env::temp_dir().join(format!("mcre_anvil_seed_legacy_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let mut bytes = vec![nbt::COMPOUND, 0, 0];
        bytes.extend_from_slice(&[nbt::COMPOUND, 0, 4]);
        bytes.extend_from_slice(b"Data");
        bytes.extend_from_slice(&[nbt::LONG, 0, 10]);
        bytes.extend_from_slice(b"RandomSeed");
        bytes.extend_from_slice(&(99i64).to_be_bytes());
        bytes.push(0);
        bytes.push(0);
        std::fs::write(dir.join("level.dat"), &bytes).unwrap();

        assert_eq!(read_seed(&dir).unwrap(), 99);

        std::fs::remove_dir_all(&dir).ok();
    }
}
