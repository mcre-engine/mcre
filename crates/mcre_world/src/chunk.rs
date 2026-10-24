use alloc::{boxed::Box, vec::Vec};

use crate::light::ChunkLight;
use crate::section::Section;
use crate::{Biome, Block, BlockState};

pub const CHUNK_WIDTH: i32 = 16;
pub const SECTION_WIDTH: usize = 16;
pub const SECTION_BLOCKS: usize = SECTION_WIDTH * SECTION_WIDTH * SECTION_WIDTH;
pub const WORLD_MIN_Y: i32 = -64;
pub const WORLD_HEIGHT: i32 = 384;
pub const SECTIONS_PER_CHUNK: usize = (WORLD_HEIGHT as usize) / SECTION_WIDTH;
pub const BIOME_CELLS_PER_SECTION: usize = 64;

fn section_index(y: i32) -> Option<usize> {
    let offset = y - WORLD_MIN_Y;
    if !(0..WORLD_HEIGHT).contains(&offset) {
        return None;
    }
    Some((offset >> 4) as usize)
}

fn biome_cell_index(local_x: i32, y: i32, local_z: i32) -> usize {
    let section_y = (y - WORLD_MIN_Y) & 15;
    let cx = ((local_x >> 2) & 3) as usize;
    let cy = ((section_y >> 2) & 3) as usize;
    let cz = ((local_z >> 2) & 3) as usize;
    (cy << 4) | (cz << 2) | cx
}

#[derive(Default, Clone)]
pub struct Chunk {
    pos: ChunkPos,
    sections: Vec<Section>,
    biomes: Vec<Option<Box<[u16; BIOME_CELLS_PER_SECTION]>>>,
    light: ChunkLight,
    stored_light: bool,
    emitters: Vec<(i32, i32, i32)>,
}

impl Chunk {
    pub fn new() -> Self {
        Self::new_at(ChunkPos::new(0, 0))
    }

    pub fn new_at(pos: ChunkPos) -> Self {
        Self {
            pos,
            sections: Vec::new(),
            biomes: Vec::new(),
            light: ChunkLight::new(),
            stored_light: false,
            emitters: Vec::new(),
        }
    }

    pub const fn pos(&self) -> ChunkPos {
        self.pos
    }

    pub fn light(&self) -> &ChunkLight {
        &self.light
    }

    pub fn light_mut(&mut self) -> &mut ChunkLight {
        &mut self.light
    }

    /// Whether this chunk's light was imported from the save (vanilla renders
    /// the stored light while it is marked on) instead of being computed.
    pub fn has_stored_light(&self) -> bool {
        self.stored_light
    }

    /// Marks the imported light as authoritative so the world skips relighting
    /// this chunk.
    pub fn mark_light_stored(&mut self) {
        self.stored_light = true;
    }

    pub fn get_block(&self, local_x: i32, y: i32, local_z: i32) -> BlockState {
        let Some(section_index) = section_index(y) else {
            return Block::AIR.default_state();
        };
        let Some(section) = self.sections.get(section_index) else {
            return Block::AIR.default_state();
        };
        section.get_block((local_x, (y - WORLD_MIN_Y) & 15, local_z))
    }

    pub fn set_block(&mut self, local_x: i32, y: i32, local_z: i32, state: BlockState) {
        debug_assert!((0..CHUNK_WIDTH).contains(&local_x), "local_x: {local_x}");
        debug_assert!((0..CHUNK_WIDTH).contains(&local_z), "local_z: {local_z}");

        let Some(section_index) = section_index(y) else {
            return;
        };

        if section_index >= self.sections.len() {
            if state.is_air() {
                return;
            }

            self.sections.resize(
                section_index + 1,
                Section::Homogeneous(Block::AIR.default_state()),
            );
        }

        let old =
            self.sections[section_index].get_block((local_x, (y - WORLD_MIN_Y) & 15, local_z));
        self.sections[section_index].set_block((local_x, (y - WORLD_MIN_Y) & 15, local_z), state);
        if old.light_emission() != state.light_emission() {
            self.emitters
                .retain(|&(x, ey, z)| x != local_x || ey != y || z != local_z);
            if state.light_emission() > 0 {
                self.emitters.push((local_x, y, local_z));
            }
        }
    }

    /// Positions (local x, world y, local z) of light-emitting blocks,
    /// maintained incrementally by `set_block` so light seeding never scans.
    pub fn emitters(&self) -> &[(i32, i32, i32)] {
        &self.emitters
    }

    pub fn biome_at(&self, local_x: i32, y: i32, local_z: i32) -> Biome {
        let Some(section_index) = section_index(y) else {
            return Biome::PLAINS;
        };
        let Some(cells) = self.biomes.get(section_index).and_then(Option::as_deref) else {
            return Biome::PLAINS;
        };
        Biome::from(cells[biome_cell_index(local_x, y, local_z)])
    }

    pub fn set_biome(&mut self, local_x: i32, y: i32, local_z: i32, biome: Biome) {
        debug_assert!((0..CHUNK_WIDTH).contains(&local_x), "local_x: {local_x}");
        debug_assert!((0..CHUNK_WIDTH).contains(&local_z), "local_z: {local_z}");

        let Some(section_index) = section_index(y) else {
            return;
        };

        if section_index >= self.biomes.len() {
            if biome == Biome::PLAINS {
                return;
            }
            self.biomes.resize(section_index + 1, None);
        }

        let cells = self.biomes[section_index]
            .get_or_insert_with(|| Box::new([u16::from(Biome::PLAINS); BIOME_CELLS_PER_SECTION]));
        cells[biome_cell_index(local_x, y, local_z)] = u16::from(biome);
    }

    pub fn fill_biome(&mut self, biome: Biome) {
        if biome == Biome::PLAINS {
            return;
        }
        self.biomes = (0..SECTIONS_PER_CHUNK)
            .map(|_| Some(Box::new([u16::from(biome); BIOME_CELLS_PER_SECTION])))
            .collect();
    }

    pub fn optimize(&mut self) {
        for section in &mut self.sections {
            section.optimize();
        }
    }

    pub fn iter_blocks(&self) -> impl Iterator<Item = (i32, i32, i32, BlockState)> + '_ {
        self.sections
            .iter()
            .enumerate()
            .filter(
                |(_, section)| !matches!(section, Section::Homogeneous(state) if state.is_air()),
            )
            .flat_map(|(index, section)| {
                let base_y = WORLD_MIN_Y + (index * SECTION_WIDTH) as i32;
                (0..SECTION_BLOCKS).filter_map(move |i| {
                    let x = i & 15;
                    let z = (i >> 4) & 15;
                    let y = (i >> 8) & 15;
                    let state = section.get_block((x as i32, y as i32, z as i32));
                    (!state.is_air()).then_some((x as i32, base_y + y as i32, z as i32, state))
                })
            })
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}

impl ChunkPos {
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    pub const fn from_block(x: i32, z: i32) -> Self {
        Self {
            x: x >> 4,
            z: z >> 4,
        }
    }

    pub const fn min_block_x(self) -> i32 {
        self.x << 4
    }

    pub const fn min_block_z(self) -> i32 {
        self.z << 4
    }
}
