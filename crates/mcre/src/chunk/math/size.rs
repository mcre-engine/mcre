use bevy::prelude::*;

use serde::{Deserialize, Serialize};

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

    pub const fn as_i64(self) -> i64 {
        self.0 as i64
    }

    pub const fn as_f32(self) -> f32 {
        self.0 as f32
    }

    pub const fn as_vec(self) -> Vec3 {
        Vec3::new(self.0 as f32, self.0 as f32, self.0 as f32)
    }
}
