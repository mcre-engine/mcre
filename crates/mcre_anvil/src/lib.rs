pub mod chunk;
pub mod nbt;
pub mod region;
pub mod resolve;

pub use chunk::{ImportRect, ImportStats, import_area, import_chunk, load_chunk};
pub use region::{RegionFile, chunk_nbt};
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
}

#[derive(Debug, Clone)]
pub struct Spawn {
    pub pos: (i32, i32, i32),
    pub dimension: String,
    pub yaw: f32,
    pub pitch: f32,
}

pub fn read_spawn(save_dir: &Path) -> Result<Spawn, Error> {
    let bytes =
        std::fs::read(save_dir.join("level.dat")).map_err(|err| Error::Io(err.to_string()))?;
    let raw = if bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
        let mut decoder = flate2::read::GzDecoder::new(&bytes[..]);
        let mut out = Vec::new();
        decoder
            .read_to_end(&mut out)
            .map_err(|err| Error::Decompress(err.to_string()))?;
        out
    } else {
        bytes
    };
    let (_, root) = nbt::read_named(&raw)?;
    let data = root.get("Data").unwrap_or(&root);
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
