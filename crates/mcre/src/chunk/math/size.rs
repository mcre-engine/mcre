use bevy::prelude::*;

use serde::{Deserialize, Serialize};

use crate::chunk::math::pos::ChunkPosition;

#[derive(Clone, Debug, PartialEq, Copy, Deserialize, Serialize)]
pub struct ChunkSize(u8);

impl ChunkSize {
    // Should probably check for above zero
    pub const fn new(size: u8) -> Self {
        ChunkSize(size)
    }

    pub const fn iter(self) -> impl Iterator<Item = u8> {
        0..self.0
    }

    pub const fn as_u8(self) -> u8 {
        self.0
    }

    pub const fn as_usize(self) -> usize {
        self.0 as usize
    }

    pub const fn as_vec(self) -> Vec3 {
        Vec3::new(self.0 as f32, self.0 as f32, self.0 as f32)
    }

    pub fn chunk_coord(self, world_coord: Vec3) -> ChunkPosition {
        ChunkPosition::into_coords(world_coord / (self.0 as f32))
    }
}
