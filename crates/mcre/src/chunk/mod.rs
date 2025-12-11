pub mod asset;
pub mod generate;
pub mod loader;
pub mod math;
pub mod mesh;

use bevy::prelude::*;
use mcre_core::BlockState;
use serde::{Deserialize, Serialize};

use crate::{
    chunk::math::{
        pos::{BlockPosition, ChunkPosition},
        size::ChunkSize,
    },
    utils::sparse::SparseVec,
};

#[derive(Asset, Clone, Debug, TypePath, Deserialize, Serialize)]
pub struct Chunk {
    pub loc: ChunkPosition,
    pub blocks: SparseVec<BlockState>,
    chunk_size: ChunkSize,
}

impl Chunk {
    pub fn empty<P: Into<ChunkPosition>>(chunk_size: ChunkSize, loc: P) -> Self {
        Chunk {
            loc: loc.into(),
            blocks: SparseVec::empty(),
            chunk_size,
        }
    }

    pub fn size(&self) -> &ChunkSize {
        &self.chunk_size
    }

    pub fn iter(&self) -> impl Iterator<Item = (usize, &BlockState)> {
        self.blocks.iter()
    }

    pub fn transform(&self) -> Transform {
        Transform::from_translation(self.loc.world_coord(self.chunk_size))
    }

    pub fn set<P: Into<BlockPosition>, B: Into<BlockState>>(&mut self, pos: P, new_block: B) {
        let pos = pos.into();
        let idx = pos.to_index(self.chunk_size);
        let new_block = new_block.into();
        if let Some(block) = self.blocks.get_mut(idx) {
            *block = new_block
        } else {
            self.blocks.insert(idx, new_block);
        }
    }

    pub fn get<P: Into<BlockPosition>>(&self, pos: P) -> Option<BlockState> {
        let index = pos.into().to_index(self.chunk_size);
        self.blocks.get(index).copied()
    }
}

impl From<Chunk> for ChunkData {
    fn from(value: Chunk) -> Self {
        ChunkData {
            loc: value.loc,
            blocks: SparseVec::from(value.blocks),
            chunk_size: value.chunk_size,
        }
    }
}

#[derive(Deserialize, Serialize)]
struct ChunkData {
    pub loc: ChunkPosition,
    pub blocks: SparseVec<u16>,
    chunk_size: ChunkSize,
}

impl From<ChunkData> for Chunk {
    fn from(value: ChunkData) -> Self {
        Chunk {
            loc: value.loc,
            blocks: SparseVec::from(value.blocks),
            chunk_size: value.chunk_size,
        }
    }
}

#[derive(Component)]
pub struct ChunkComponent(pub Handle<Chunk>);
