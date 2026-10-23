use alloc::boxed::Box;

use mcre_core::Direction;

use crate::{BlockState, WORLD_HEIGHT, WORLD_MIN_Y, shape};

pub const ALL_DIRECTIONS: [Direction; 6] = [
    Direction::Down,
    Direction::Up,
    Direction::North,
    Direction::South,
    Direction::West,
    Direction::East,
];

/// Bytes per light channel in one 16^3 section (two 4-bit cells per byte).
pub const SECTION_LIGHT_BYTES: usize = 2048;

fn nibble_index(local_x: i32, y: i32, local_z: i32) -> Option<usize> {
    let offset = y - WORLD_MIN_Y;
    if !(0..WORLD_HEIGHT).contains(&offset)
        || !(0..16).contains(&local_x)
        || !(0..16).contains(&local_z)
    {
        return None;
    }
    let local_y = offset & 15;
    Some(((local_y as usize) << 8) | ((local_z as usize) << 4) | local_x as usize)
}

fn get_nibble(data: &[u8; SECTION_LIGHT_BYTES], index: usize) -> u8 {
    let byte = data[index >> 1];
    if index & 1 == 0 {
        byte >> 4
    } else {
        byte & 0x0F
    }
}

fn set_nibble(data: &mut [u8; SECTION_LIGHT_BYTES], index: usize, level: u8) {
    let byte = &mut data[index >> 1];
    if index & 1 == 0 {
        *byte = (*byte & 0x0F) | (level << 4);
    } else {
        *byte = (*byte & 0xF0) | level;
    }
}

/// Block and sky light for one 16^3 section as packed nibbles (2048 bytes
/// per channel).
#[derive(Clone)]
pub struct SectionLight {
    block: [u8; SECTION_LIGHT_BYTES],
    sky: [u8; SECTION_LIGHT_BYTES],
}

impl SectionLight {
    fn new() -> Self {
        Self {
            block: [0; SECTION_LIGHT_BYTES],
            sky: [0; SECTION_LIGHT_BYTES],
        }
    }
}

/// Per-chunk light storage: sections are allocated lazily on first write.
#[derive(Clone)]
pub struct ChunkLight {
    sections: [Option<Box<SectionLight>>; crate::SECTIONS_PER_CHUNK],
}

impl Default for ChunkLight {
    fn default() -> Self {
        Self {
            sections: core::array::from_fn(|_| None),
        }
    }
}

impl ChunkLight {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces one section's light with the save's nibble arrays (2048 bytes
    /// per channel). Vanilla packs the even cell index in the low nibble and
    /// the odd one in the high nibble, the opposite of this module, so the
    /// bytes are rotated on the way in. Returns `false` for a section outside
    /// the world or arrays of the wrong length.
    pub fn set_section_from_save(&mut self, base_y: i32, sky: &[u8], block: &[u8]) -> bool {
        if sky.len() != SECTION_LIGHT_BYTES || block.len() != SECTION_LIGHT_BYTES {
            return false;
        }
        let Some(section) = self.section_mut(base_y, true) else {
            return false;
        };
        for (dst, src) in section.sky.iter_mut().zip(sky) {
            *dst = src.rotate_left(4);
        }
        for (dst, src) in section.block.iter_mut().zip(block) {
            *dst = src.rotate_left(4);
        }
        true
    }

    fn section(&self, y: i32) -> Option<&SectionLight> {
        let offset = y - WORLD_MIN_Y;
        if !(0..WORLD_HEIGHT).contains(&offset) {
            return None;
        }
        self.sections[(offset >> 4) as usize].as_deref()
    }

    fn section_mut(&mut self, y: i32, allocate: bool) -> Option<&mut SectionLight> {
        let offset = y - WORLD_MIN_Y;
        if !(0..WORLD_HEIGHT).contains(&offset) {
            return None;
        }
        let slot = &mut self.sections[(offset >> 4) as usize];
        if allocate && slot.is_none() {
            *slot = Some(Box::new(SectionLight::new()));
        }
        slot.as_deref_mut()
    }

    /// Sky light level (0..=15); unlit sections read as 0.
    pub fn sky_level(&self, local_x: i32, y: i32, local_z: i32) -> u8 {
        let (Some(index), Some(section)) = (nibble_index(local_x, y, local_z), self.section(y))
        else {
            return 0;
        };
        get_nibble(&section.sky, index)
    }

    /// Block light level (0..=15); unlit sections read as 0.
    pub fn block_level(&self, local_x: i32, y: i32, local_z: i32) -> u8 {
        let (Some(index), Some(section)) = (nibble_index(local_x, y, local_z), self.section(y))
        else {
            return 0;
        };
        get_nibble(&section.block, index)
    }

    /// Combined `(block, sky)` light value for a block position; unlit
    /// sections read as 0.
    pub fn level(&self, local_x: i32, y: i32, local_z: i32) -> (u8, u8) {
        (
            self.block_level(local_x, y, local_z),
            self.sky_level(local_x, y, local_z),
        )
    }

    pub fn set_sky_level(&mut self, local_x: i32, y: i32, local_z: i32, level: u8) {
        let Some(index) = nibble_index(local_x, y, local_z) else {
            return;
        };
        if level == 0
            && self
                .sections
                .get(((y - WORLD_MIN_Y) >> 4) as usize)
                .is_none_or(|s| s.is_none())
        {
            return;
        }
        if let Some(section) = self.section_mut(y, level > 0) {
            set_nibble(&mut section.sky, index, level & 0x0F);
        }
    }

    pub fn set_block_level(&mut self, local_x: i32, y: i32, local_z: i32, level: u8) {
        let Some(index) = nibble_index(local_x, y, local_z) else {
            return;
        };
        if level == 0
            && self
                .sections
                .get(((y - WORLD_MIN_Y) >> 4) as usize)
                .is_none_or(|s| s.is_none())
        {
            return;
        }
        if let Some(section) = self.section_mut(y, level > 0) {
            set_nibble(&mut section.block, index, level & 0x0F);
        }
    }
}

/// Vanilla `LightEngine` decay: skylight falls freely downward from a full
/// level-15 column while the pair does not shape-occlude; everything else
/// loses `max(1, damp)`.
pub(crate) fn decay(from: u8, target: BlockState, direction: Direction, sky: bool) -> u8 {
    let damp = target.light_dampening();
    if sky && direction == Direction::Down && from == 15 && damp == 0 {
        0
    } else {
        damp.max(1)
    }
}

pub(crate) fn propagate(
    from: u8,
    from_state: BlockState,
    target: BlockState,
    direction: Direction,
    sky: bool,
) -> u8 {
    let next = from.saturating_sub(decay(from, target, direction, sky));
    if next == 0 {
        return 0;
    }
    if shape::shape_occludes(from_state, target, direction) {
        return 0;
    }
    next
}
