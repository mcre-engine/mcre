pub mod asset;
pub mod generate;
pub mod loader;
pub mod math;
pub mod mesh;

use bevy::{platform::collections::HashMap, prelude::*};
use mcre_core::{Block, BlockState};
use serde::{Deserialize, Serialize};

use crate::chunk::math::{
    pos::{BlockPosition, ChunkPosition},
    size::ChunkSize,
};

#[derive(Asset, Clone, Debug, TypePath, Deserialize, Serialize)]
pub struct Chunk {
    pub loc: ChunkPosition,
    sections: HashMap<i64, Section>,
    chunk_size: ChunkSize,
}

impl Chunk {
    pub fn empty<P: Into<ChunkPosition>>(chunk_size: ChunkSize, loc: P) -> Self {
        Chunk {
            loc: loc.into(),
            sections: HashMap::new(),
            chunk_size,
        }
    }

    pub fn size(&self) -> &ChunkSize {
        &self.chunk_size
    }

    pub fn iter(&self) -> impl Iterator<Item = (BlockPosition, BlockState)> {
        BlockIterator {
            section_iter: self.sections.iter(),
            chunk_size: self.chunk_size,
            cur_section: None,
        }
    }

    pub fn transform(&self) -> Transform {
        Transform::from_translation(self.loc.into_world_coord(self.chunk_size))
    }

    pub fn set<P: Into<BlockPosition>, B: Into<BlockState>>(&mut self, pos: P, new_block: B) {
        let (key, index) = key_index(self.chunk_size, pos.into());
        let section = self
            .sections
            .entry(key)
            .or_insert_with(|| Section::new(self.chunk_size));
        section.blocks[index] = new_block.into();
    }

    pub fn get<P: Into<BlockPosition>>(&self, pos: P) -> Option<BlockState> {
        let (key, index) = key_index(self.chunk_size, pos.into());
        let section = self.sections.get(&key)?;
        Some(section.blocks[index])
    }
}
fn key_index(chunk_size: ChunkSize, pos: BlockPosition) -> (i64, usize) {
    let key = pos.y as f64 / chunk_size.as_f32() as f64;
    let key = key.floor() as i64;
    let size = chunk_size.as_usize();
    let y_diff = pos.y.unsigned_abs() as usize % size;
    let index = pos.x as usize * size * size + y_diff * size + pos.z as usize;
    (key, index)
}

#[derive(Component)]
pub struct ChunkComponent(pub Handle<Chunk>);

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Section {
    blocks: Box<[BlockState]>,
}

impl Section {
    fn new(chunk_size: ChunkSize) -> Self {
        Section {
            blocks: vec![Block::AIR.into(); chunk_size.as_usize().pow(3)].into(),
        }
    }
}

pub struct BlockIterator<'a, T> {
    section_iter: T,
    chunk_size: ChunkSize,
    cur_section: Option<(i64, &'a Section, BlockPosition)>,
}

impl<'a, T: Iterator<Item = (&'a i64, &'a Section)>> Iterator for BlockIterator<'a, T> {
    type Item = (BlockPosition, BlockState);

    fn next(&mut self) -> Option<Self::Item> {
        let (y, section, mut pos) = match self.cur_section.take() {
            None => {
                let (y, section) = self.section_iter.next()?;
                (
                    *y,
                    section,
                    BlockPosition {
                        x: 0,
                        y: *y * self.chunk_size.as_i64(),
                        z: 0,
                    },
                )
            }
            Some(cur) => cur,
        };
        let (_key, index) = key_index(self.chunk_size, pos);
        let block = section.blocks[index];
        let out = pos;
        if pos.x < self.chunk_size.as_u8() - 1 {
            pos.x += 1;
            self.cur_section = Some((y, section, pos));
        } else if pos.z < self.chunk_size.as_u8() - 1 {
            pos.x = 0;
            pos.z += 1;
            self.cur_section = Some((y, section, pos));
        } else if pos.y < ((y + 1) * self.chunk_size.as_i64() - 1) {
            pos.x = 0;
            pos.z = 0;
            pos.y += 1;
            self.cur_section = Some((y, section, pos));
        } else {
            self.cur_section = None;
        }
        Some((out, block))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_index() {
        let size = ChunkSize::new(16);
        let (key, index) = key_index(size, BlockPosition { x: 0, y: -2, z: 0 });
        assert_eq!(key, -1);
        assert_eq!(index, 2 * size.as_usize());

        let (key, index) = key_index(size, BlockPosition { x: 0, y: 2, z: 0 });
        assert_eq!(key, 0);
        assert_eq!(index, 2 * size.as_usize());
    }

    #[test]
    fn test_iter() {
        let size = ChunkSize::new(16);
        let mut chunk = Chunk::empty(size, ChunkPosition::from_world_coord(Vec3::splat(0.), size));
        chunk.set((0, -2, 0), Block::STONE);
        let mut iter = chunk.iter();

        assert_eq!(
            iter.next(),
            Some((
                BlockPosition { x: 0, y: -16, z: 0 },
                BlockState::from(Block::AIR)
            ))
        );
        assert_eq!(
            iter.next(),
            Some((
                BlockPosition { x: 1, y: -16, z: 0 },
                BlockState::from(Block::AIR)
            ))
        );
        let (p, _) = iter
            .find(|(_, b)| *b == BlockState::from(Block::STONE))
            .unwrap();
        assert_eq!(p, (0, -2, 0));
    }
}
