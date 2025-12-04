use bevy::prelude::*;

use serde::{Deserialize, Serialize};

use crate::chunk::math::pos::ChunkPosition;

#[derive(Clone, Debug, PartialEq, Copy, Deserialize, Serialize)]
pub struct ChunkSize(usize);

impl ChunkSize {
    // Should probably check for above zero
    pub const fn new(size: usize) -> Self {
        ChunkSize(size)
    }

    pub const fn iter(self) -> impl Iterator<Item = usize> {
        0..self.0
    }

    pub const fn as_u32(self) -> u32 {
        self.0 as u32
    }

    pub const fn in_bounds(self, p: UVec3) -> BVec3 {
        BVec3::new(
            (p.x as usize) < self.0,
            (p.y as usize) < self.0,
            (p.z as usize) < self.0,
        )
    }

    pub const fn as_vec(self) -> Vec3 {
        Vec3::new(self.0 as f32, self.0 as f32, self.0 as f32)
    }

    pub fn chunk_coord(self, world_coord: Vec3) -> ChunkPosition {
        ChunkPosition::into_coords(world_coord / (self.0 as f32))
    }

    pub const fn full_size(self) -> usize {
        self.0.pow(3)
    }

    // Index of block inside the chunk array based off this size
    // These are local coordinates
    pub fn chunk_index(self, pos: UVec3) -> usize {
        pos.x as usize * self.0.pow(2) + pos.y as usize * self.0 + pos.z as usize
    }

    // X, Y, Z position of the block local to the chunk
    pub fn block_index(self, mut index: usize) -> UVec3 {
        let d = self.0.pow(2);
        let x = index / d;
        index %= d;
        let d = self.0;
        let y = index / d;
        index %= d;
        UVec3::new(x as u32, y as u32, index as u32)
    }
}
