use mcre_world::light::SECTION_LIGHT_BYTES;
use mcre_world::{Biome, BlockState, Chunk, ChunkPos, WORLD_MIN_Y, World};

use crate::Error;
use crate::nbt::Tag;
use crate::region;
use crate::resolve::{BiomeResolver, BlockResolver};

pub const SECTION_CELLS: usize = 4096;
pub const BIOME_ENTRIES: usize = 4096;

#[derive(Debug, Default)]
pub struct ImportStats {
    pub chunks: u32,
    pub sections: u32,
    pub blocks_set: u64,
    pub skipped_sections: u32,
    pub skipped_chunks: u32,
    pub unknown_blocks: Vec<String>,
    pub unknown_biomes: Vec<String>,
}

impl ImportStats {
    fn record_unknown_block(&mut self, name: &str) {
        if self.unknown_blocks.len() < 32 && !self.unknown_blocks.iter().any(|n| n == name) {
            self.unknown_blocks.push(name.to_string());
        }
    }

    fn record_unknown_biome(&mut self, name: &str) {
        if self.unknown_biomes.len() < 32 && !self.unknown_biomes.iter().any(|n| n == name) {
            self.unknown_biomes.push(name.to_string());
        }
    }
}

fn ceil_log2(len: usize) -> u32 {
    (len - 1).ilog2() + 1
}

fn unpack(data: &[i64], bits: u32, count: usize) -> Result<Vec<usize>, Error> {
    if bits == 0 || bits > 32 {
        return Err(Error::BitStorageTooShort);
    }
    let values_per_long = 64 / bits as usize;
    let required = count.div_ceil(values_per_long);
    if data.len() != required {
        return Err(Error::BitStorageLength {
            expected: required,
            found: data.len(),
        });
    }
    let mask = (1u64 << bits) - 1;
    let mut out = Vec::with_capacity(count);
    for index in 0..count {
        let word = index / values_per_long;
        let offset = ((index % values_per_long) as u32) * bits;
        let value = (data[word] as u64 >> offset) & mask;
        out.push(value as usize);
    }
    Ok(out)
}

fn palette_entry_state(
    entry: &Tag,
    blocks: &BlockResolver,
    stats: &mut ImportStats,
) -> Option<BlockState> {
    match entry {
        Tag::String(name) => {
            let state = blocks.resolve(name, &[]);
            if state.is_none() {
                stats.record_unknown_block(name);
            }
            state
        }
        Tag::Compound(_) => {
            let name = entry
                .get("id")
                .or_else(|| entry.get("Name"))
                .and_then(Tag::as_str)
                .or_else(|| {
                    if let Tag::Compound(fields) = entry
                        && fields.len() == 1
                        && fields[0].0.is_empty()
                    {
                        fields[0].1.as_str()
                    } else {
                        None
                    }
                });
            let Some(name) = name else {
                stats.record_unknown_block("<malformed palette entry>");
                return None;
            };
            let mut properties = Vec::new();
            if let Some(props) = entry.get("properties").or_else(|| entry.get("Properties")) {
                let Tag::Compound(fields) = props else {
                    stats.record_unknown_block(name);
                    return None;
                };
                for (key, value) in fields {
                    let Tag::String(value) = value else {
                        stats.record_unknown_block(name);
                        return None;
                    };
                    properties.push((key.clone(), value.clone()));
                }
            }
            let state = blocks.resolve(name, &properties);
            if state.is_none() {
                let label = if properties.is_empty() {
                    name.to_string()
                } else {
                    format!("{name} {properties:?}")
                };
                stats.record_unknown_block(&label);
            }
            state
        }
        _ => {
            stats.record_unknown_block("<non-string palette entry>");
            None
        }
    }
}

struct DecodedSection {
    base_y: i32,
    blocks: Vec<(i32, i32, i32, BlockState)>,
    biomes: Vec<(i32, i32, i32, Biome)>,
}

fn decode_section(
    section: &Tag,
    blocks: &BlockResolver,
    biomes: &BiomeResolver,
    stats: &mut ImportStats,
) -> Result<Option<DecodedSection>, Error> {
    let section_y = section
        .get("Y")
        .and_then(Tag::as_i32)
        .ok_or(Error::MissingField("section.Y"))?;
    let min_section_y = WORLD_MIN_Y.div_euclid(16);
    let section_index = section_y - min_section_y;
    if section_index < 0 || section_index as usize >= mcre_world::SECTIONS_PER_CHUNK {
        stats.skipped_sections += 1;
        return Ok(None);
    }
    let base_y = WORLD_MIN_Y + section_index * 16;
    let mut decoded = DecodedSection {
        base_y,
        blocks: Vec::new(),
        biomes: Vec::new(),
    };

    if let Some(block_states) = section.get("block_states") {
        let palette = block_states
            .get("palette")
            .and_then(Tag::as_list)
            .ok_or(Error::MissingField("block_states.palette"))?;
        if palette.is_empty() {
            return Err(Error::MissingField("block_states.palette"));
        }
        let resolved: Vec<Option<BlockState>> = palette
            .iter()
            .map(|entry| palette_entry_state(entry, blocks, stats))
            .collect();
        if resolved.len() == 1 {
            if let Some(state) = resolved[0].filter(|state| !state.is_air()) {
                for y in 0..16 {
                    for z in 0..16 {
                        for x in 0..16 {
                            decoded.blocks.push((x, y, z, state));
                        }
                    }
                }
            }
        } else {
            let bits = ceil_log2(resolved.len()).max(4);
            let data = block_states
                .get("data")
                .and_then(Tag::as_i64_array)
                .ok_or(Error::MissingField("block_states.data"))?;
            let indices = unpack(&data, bits, SECTION_CELLS)?;
            for (index, palette_index) in indices.iter().enumerate() {
                let state = resolved
                    .get(*palette_index)
                    .copied()
                    .flatten()
                    .ok_or(Error::PaletteIndex(*palette_index))?;
                if state.is_air() {
                    continue;
                }
                let x = (index & 15) as i32;
                let z = ((index >> 4) & 15) as i32;
                let y = ((index >> 8) & 15) as i32;
                decoded.blocks.push((x, y, z, state));
            }
        }
    }

    if let Some(biome_container) = section.get("biomes") {
        let palette = biome_container
            .get("palette")
            .and_then(Tag::as_list)
            .ok_or(Error::MissingField("biomes.palette"))?;
        if palette.is_empty() {
            return Err(Error::MissingField("biomes.palette"));
        }
        let mut resolved: Vec<Option<Biome>> = Vec::with_capacity(palette.len());
        for entry in palette {
            let name = entry.as_str().or_else(|| {
                entry
                    .get("id")
                    .or_else(|| entry.get("Name"))
                    .and_then(Tag::as_str)
            });
            match name.and_then(|name| biomes.resolve(name)) {
                Some(biome) => resolved.push(Some(biome)),
                None => {
                    stats.record_unknown_biome(name.unwrap_or("<malformed>"));
                    resolved.push(None);
                }
            }
        }
        let indices: Vec<usize> = if resolved.len() == 1 {
            vec![0; BIOME_ENTRIES]
        } else {
            let bits = ceil_log2(resolved.len());
            let data = biome_container
                .get("data")
                .and_then(Tag::as_i64_array)
                .ok_or(Error::MissingField("biomes.data"))?;
            // Entry count changed across versions: 26.4 stores per-block
            // (4096) biome entries, older chunks store 4x4x4 cells (64).
            // Both use the same YZX order and legacy bit layout; the data
            // length disambiguates (the two lengths never coincide).
            let values_per_long = 64 / bits as usize;
            if data.len() == BIOME_ENTRIES.div_ceil(values_per_long) {
                unpack(&data, bits, BIOME_ENTRIES)?
            } else if data.len() == 64usize.div_ceil(values_per_long) {
                // Pre-26.4 4x4x4 cells in YZX order: expand each cell over
                // its 4x4x4 block range so the majority vote below sees the
                // same layout as per-block data.
                let cells = unpack(&data, bits, 64)?;
                let mut expanded = Vec::with_capacity(BIOME_ENTRIES);
                for y in 0..16 {
                    for z in 0..16 {
                        for x in 0..16 {
                            let cell = (x >> 2) + (z >> 2) * 4 + (y >> 2) * 16;
                            expanded.push(cells[cell]);
                        }
                    }
                }
                expanded
            } else {
                return Err(Error::BitStorageLength {
                    expected: BIOME_ENTRIES.div_ceil(values_per_long),
                    found: data.len(),
                });
            }
        };
        let mut cell_votes: Vec<Vec<(usize, u32)>> = vec![Vec::new(); 64];
        for (index, palette_index) in indices.iter().enumerate() {
            if resolved.get(*palette_index).copied().flatten().is_none() {
                continue;
            }
            let x = (index & 15) as i32;
            let z = ((index >> 4) & 15) as i32;
            let y = ((index >> 8) & 15) as i32;
            let cell = ((x >> 2) + (z >> 2) * 4 + (y >> 2) * 16) as usize;
            match cell_votes[cell]
                .iter_mut()
                .find(|(idx, _)| *idx == *palette_index)
            {
                Some((_, count)) => *count += 1,
                None => cell_votes[cell].push((*palette_index, 1)),
            }
        }
        for (cell, votes) in cell_votes.iter().enumerate() {
            let Some((winner, _)) = votes.iter().max_by_key(|(_, count)| *count) else {
                continue;
            };
            let Some(biome) = resolved[*winner] else {
                continue;
            };
            let cell_x = (cell & 3) as i32;
            let cell_z = ((cell >> 2) & 3) as i32;
            let cell_y = ((cell >> 4) & 3) as i32;
            decoded
                .biomes
                .push((cell_x * 4, cell_y * 4, cell_z * 4, biome));
        }
    }

    stats.sections += 1;
    Ok(Some(decoded))
}

fn import_section(
    world: &mut World,
    chunk_x: i32,
    chunk_z: i32,
    section: &Tag,
    blocks: &BlockResolver,
    biomes: &BiomeResolver,
    stats: &mut ImportStats,
) -> Result<(), Error> {
    let Some(decoded) = decode_section(section, blocks, biomes, stats)? else {
        return Ok(());
    };
    for (x, y, z, state) in decoded.blocks {
        world.set_block(
            (chunk_x * 16 + x, decoded.base_y + y, chunk_z * 16 + z),
            state,
        );
        stats.blocks_set += 1;
    }
    for (x, y, z, biome) in decoded.biomes {
        world.set_biome(
            (chunk_x * 16 + x, decoded.base_y + y, chunk_z * 16 + z),
            biome,
        );
    }
    Ok(())
}

fn import_sections(
    world: &mut World,
    chunk_x: i32,
    chunk_z: i32,
    sections: &[Tag],
    blocks: &BlockResolver,
    biomes: &BiomeResolver,
    stats: &mut ImportStats,
) -> Result<(), Error> {
    for section in sections {
        import_section(world, chunk_x, chunk_z, section, blocks, biomes, stats)?;
    }
    stats.chunks += 1;
    Ok(())
}

pub fn import_chunk(
    world: &mut World,
    chunk_x: i32,
    chunk_z: i32,
    root: &Tag,
    blocks: &BlockResolver,
    biomes: &BiomeResolver,
    stats: &mut ImportStats,
) -> Result<(), Error> {
    if let (Some(x), Some(z)) = (
        root.get("xPos").and_then(Tag::as_i32),
        root.get("zPos").and_then(Tag::as_i32),
    ) && (x != chunk_x || z != chunk_z)
    {
        return Err(Error::ChunkMismatch {
            expected: (chunk_x, chunk_z),
            found: (x, z),
        });
    }
    let Some(sections) = root.get("sections").and_then(Tag::as_list) else {
        stats.skipped_chunks += 1;
        return Ok(());
    };
    import_sections(world, chunk_x, chunk_z, sections, blocks, biomes, stats)
}

#[derive(Debug, Clone, Copy)]
pub struct ImportRect {
    pub min_chunk: (i32, i32),
    pub max_chunk: (i32, i32),
    pub origin_chunk: (i32, i32),
}

/// Loads one chunk from a region directory as a detached `Chunk` at true
/// save coordinates, including the light stored in the save when it is marked
/// correct (`isLightOn`). Returns `None` when the chunk is not stored (caller
/// typically substitutes an empty air chunk).
pub fn load_chunk(
    region_dir: &std::path::Path,
    chunk_x: i32,
    chunk_z: i32,
    blocks: &BlockResolver,
    biomes: &BiomeResolver,
    stats: &mut ImportStats,
) -> Result<Option<Chunk>, Error> {
    let Some(root) = region::chunk_nbt(region_dir, chunk_x, chunk_z)? else {
        stats.skipped_chunks += 1;
        return Ok(None);
    };
    load_chunk_from_nbt(&root, chunk_x, chunk_z, blocks, biomes, stats)
}

/// Loads one chunk from an already-parsed chunk NBT root. The browser save
/// loader reads region bytes through OPFS and calls this directly.
pub fn load_chunk_from_nbt(
    root: &Tag,
    chunk_x: i32,
    chunk_z: i32,
    blocks: &BlockResolver,
    biomes: &BiomeResolver,
    stats: &mut ImportStats,
) -> Result<Option<Chunk>, Error> {
    let Some(sections) = root.get("sections").and_then(Tag::as_list) else {
        stats.skipped_chunks += 1;
        return Ok(None);
    };
    let light_correct = root.get("isLightOn").and_then(Tag::as_i32) == Some(1);
    let mut chunk = Chunk::new_at(ChunkPos::new(chunk_x, chunk_z));
    let mut sky_layers: Vec<Option<[u8; SECTION_LIGHT_BYTES]>> =
        vec![None; mcre_world::SECTIONS_PER_CHUNK];
    let mut block_layers: Vec<Option<[u8; SECTION_LIGHT_BYTES]>> =
        vec![None; mcre_world::SECTIONS_PER_CHUNK];
    for section in sections {
        let Some(decoded) = decode_section(section, blocks, biomes, stats)? else {
            continue;
        };
        let section_index = (decoded.base_y - WORLD_MIN_Y).div_euclid(16) as usize;
        if light_correct && section_index < mcre_world::SECTIONS_PER_CHUNK {
            sky_layers[section_index] = section_light_bytes(section, "SkyLight");
            block_layers[section_index] = section_light_bytes(section, "BlockLight");
        }
        for (x, y, z, state) in decoded.blocks {
            chunk.set_block(x, decoded.base_y + y, z, state);
            stats.blocks_set += 1;
        }
        for (x, y, z, biome) in decoded.biomes {
            chunk.set_biome(x, decoded.base_y + y, z, biome);
        }
    }
    if light_correct {
        import_chunk_light(&mut chunk, &sky_layers, &block_layers);
        chunk.mark_light_stored();
    }
    stats.chunks += 1;
    Ok(Some(chunk))
}

/// Decodes one section's nibble light array, if the section stores it.
fn section_light_bytes(section: &Tag, key: &str) -> Option<[u8; SECTION_LIGHT_BYTES]> {
    let values = section.get(key).and_then(Tag::as_i8_array)?;
    if values.len() != SECTION_LIGHT_BYTES {
        return None;
    }
    let mut bytes = [0u8; SECTION_LIGHT_BYTES];
    for (dst, src) in bytes.iter_mut().zip(values) {
        *dst = *src as u8;
    }
    Some(bytes)
}

/// Fills every section's light the way vanilla's light storage reads a save
/// (`SkyLightSectionStorage`/`BlockLightSectionStorage`): sections above the
/// topmost stored sky layer are fully lit, missing sky layers inherit the
/// nearest layer above them, and missing block layers are dark.
fn import_chunk_light(
    chunk: &mut Chunk,
    sky_layers: &[Option<[u8; SECTION_LIGHT_BYTES]>],
    block_layers: &[Option<[u8; SECTION_LIGHT_BYTES]>],
) {
    let mut sky = [0xFFu8; SECTION_LIGHT_BYTES];
    let mut block = [0u8; SECTION_LIGHT_BYTES];
    for index in (0..mcre_world::SECTIONS_PER_CHUNK).rev() {
        if let Some(layer) = sky_layers[index] {
            sky = layer;
        }
        if let Some(layer) = block_layers[index] {
            block = layer;
        }
        let base_y = WORLD_MIN_Y + index as i32 * 16;
        chunk
            .light_mut()
            .set_section_from_save(base_y, &sky, &block);
    }
}

pub fn import_area(
    world: &mut World,
    region_dir: &std::path::Path,
    rect: ImportRect,
    blocks: &BlockResolver,
    biomes: &BiomeResolver,
    stats: &mut ImportStats,
) -> Result<(), Error> {
    for chunk_z in rect.min_chunk.1..=rect.max_chunk.1 {
        for chunk_x in rect.min_chunk.0..=rect.max_chunk.0 {
            let Some(root) = region::chunk_nbt(region_dir, chunk_x, chunk_z)? else {
                stats.skipped_chunks += 1;
                continue;
            };
            let Some(sections) = root.get("sections").and_then(Tag::as_list) else {
                stats.skipped_chunks += 1;
                continue;
            };
            import_sections(
                world,
                chunk_x - rect.origin_chunk.0,
                chunk_z - rect.origin_chunk.1,
                sections,
                blocks,
                biomes,
                stats,
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use std::io::Write;

    fn field(out: &mut Vec<u8>, kind: u8, name: &[u8]) {
        out.push(kind);
        out.extend_from_slice(&(name.len() as u16).to_be_bytes());
        out.extend_from_slice(name);
    }

    fn chunk_payload() -> Vec<u8> {
        let mut out = vec![10, 0, 0];
        field(&mut out, 3, b"xPos");
        out.extend_from_slice(&5i32.to_be_bytes());
        field(&mut out, 3, b"zPos");
        out.extend_from_slice(&(-3i32).to_be_bytes());
        field(&mut out, 9, b"sections");
        out.push(10);
        out.extend_from_slice(&1i32.to_be_bytes());
        out.push(1);
        out.extend_from_slice(&[0, 1, b'Y', 0]);
        field(&mut out, 10, b"block_states");
        field(&mut out, 9, b"palette");
        out.push(8);
        out.extend_from_slice(&1i32.to_be_bytes());
        let name = b"minecraft:stone";
        out.extend_from_slice(&(name.len() as u16).to_be_bytes());
        out.extend_from_slice(name);
        out.push(0);
        field(&mut out, 10, b"biomes");
        field(&mut out, 9, b"palette");
        out.push(8);
        out.extend_from_slice(&1i32.to_be_bytes());
        let biome = b"minecraft:plains";
        out.extend_from_slice(&(biome.len() as u16).to_be_bytes());
        out.extend_from_slice(biome);
        out.push(0);
        out.push(0);
        out.push(0);
        out
    }

    fn write_region(dir: &std::path::Path, payload: &[u8]) {
        let mut compressed = Vec::new();
        ZlibEncoder::new(&mut compressed, Compression::default())
            .write_all(payload)
            .unwrap();
        let sectors = (5 + compressed.len()).div_ceil(4096);
        let mut bytes = vec![0u8; 8192 + sectors * 4096];
        let index = (5 + 29 * 32) * 4;
        let entry = (2u32 << 8) | sectors as u32;
        bytes[index..index + 4].copy_from_slice(&entry.to_be_bytes());
        bytes[8192..8196].copy_from_slice(&((compressed.len() + 1) as u32).to_be_bytes());
        bytes[8196] = 2;
        bytes[8197..8197 + compressed.len()].copy_from_slice(&compressed);
        std::fs::write(dir.join("r.0.-1.mca"), bytes).unwrap();
    }

    #[test]
    fn load_chunk_assembles_detached_chunk() {
        let dir = std::env::temp_dir().join("mcre_anvil_load_chunk_test");
        std::fs::create_dir_all(&dir).unwrap();
        write_region(&dir, &chunk_payload());

        let blocks = BlockResolver::new();
        let biomes = BiomeResolver::new();
        let mut stats = ImportStats::default();
        let chunk = load_chunk(&dir, 5, -3, &blocks, &biomes, &mut stats)
            .expect("load")
            .expect("present");
        assert_eq!(stats.chunks, 1);
        assert_eq!((chunk.pos().x, chunk.pos().z), (5, -3));
        assert_eq!(chunk.get_block(0, 5, 0).block().name(), "stone",);
        assert_eq!(chunk.biome_at(0, 5, 0).name(), "plains");
        assert!(
            load_chunk(&dir, 6, -3, &blocks, &biomes, &mut stats)
                .expect("load")
                .is_none()
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_chunk_from_parsed_nbt() {
        let (_, root) = crate::nbt::read_named(&chunk_payload()).unwrap();
        let blocks = BlockResolver::new();
        let biomes = BiomeResolver::new();
        let mut stats = ImportStats::default();
        let chunk = load_chunk_from_nbt(&root, 5, -3, &blocks, &biomes, &mut stats)
            .expect("load")
            .expect("present");
        assert_eq!((chunk.pos().x, chunk.pos().z), (5, -3));
        assert_eq!(chunk.get_block(0, 5, 0).block().name(), "stone");
        assert_eq!(chunk.biome_at(0, 5, 0).name(), "plains");
    }

    fn chunk_payload_legacy_biomes() -> Vec<u8> {
        let mut out = vec![10, 0, 0];
        field(&mut out, 3, b"xPos");
        out.extend_from_slice(&5i32.to_be_bytes());
        field(&mut out, 3, b"zPos");
        out.extend_from_slice(&(-3i32).to_be_bytes());
        field(&mut out, 9, b"sections");
        out.push(10);
        out.extend_from_slice(&1i32.to_be_bytes());
        out.push(1);
        out.extend_from_slice(&[0, 1, b'Y', 0]);
        field(&mut out, 10, b"block_states");
        field(&mut out, 9, b"palette");
        out.push(8);
        out.extend_from_slice(&1i32.to_be_bytes());
        let name = b"minecraft:stone";
        out.extend_from_slice(&(name.len() as u16).to_be_bytes());
        out.extend_from_slice(name);
        out.push(0);
        // Pre-26.4 biome container: 2-entry palette with a single long
        // (64 cells, 1 bit each, all selecting palette index 1).
        field(&mut out, 10, b"biomes");
        field(&mut out, 9, b"palette");
        out.push(8);
        out.extend_from_slice(&2i32.to_be_bytes());
        for biome in [
            b"minecraft:plains".as_slice(),
            b"minecraft:forest".as_slice(),
        ] {
            out.extend_from_slice(&(biome.len() as u16).to_be_bytes());
            out.extend_from_slice(biome);
        }
        field(&mut out, 12, b"data");
        out.extend_from_slice(&1i32.to_be_bytes());
        out.extend_from_slice(&(-1i64).to_be_bytes());
        out.push(0);
        out.push(0);
        out.push(0);
        out
    }

    #[test]
    fn load_chunk_reads_legacy_64_cell_biomes() {
        let dir = std::env::temp_dir().join("mcre_anvil_legacy_biome_test");
        std::fs::create_dir_all(&dir).unwrap();
        write_region(&dir, &chunk_payload_legacy_biomes());

        let blocks = BlockResolver::new();
        let biomes = BiomeResolver::new();
        let mut stats = ImportStats::default();
        let chunk = load_chunk(&dir, 5, -3, &blocks, &biomes, &mut stats)
            .expect("load")
            .expect("present");
        assert_eq!(chunk.biome_at(0, 5, 0).name(), "forest");
        assert_eq!(chunk.biome_at(15, 5, 15).name(), "forest");

        std::fs::remove_dir_all(&dir).ok();
    }
}
