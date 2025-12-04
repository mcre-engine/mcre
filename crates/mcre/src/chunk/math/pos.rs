use std::ops::Deref;

use bevy::math::{I64Vec3, Vec3};
use serde::{Deserialize, Serialize};

use crate::chunk::math::size::ChunkSize;

/// Chunk's position in world (or relative world) coordinates
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Deserialize, Serialize)]
pub struct ChunkPosition(I64Vec3);

impl ChunkPosition {
    pub fn into_coords(pos: Vec3) -> Self {
        let chunk = pos.floor();
        ChunkPosition(I64Vec3::new(chunk.x as i64, chunk.y as i64, chunk.z as i64))
    }

    pub fn world_coord(self, size: ChunkSize) -> Vec3 {
        size.as_vec() * self.0.as_vec3()
    }

    pub fn iter_around(self, radius: u64) -> impl Iterator<Item = Self> {
        ChunkIterator::new(self, radius as i64)
    }
}

impl Deref for ChunkPosition {
    type Target = I64Vec3;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct ChunkIterator {
    radius: i64,
    around: ChunkPosition,
    next_pos: Option<ChunkPosition>,
}

impl ChunkIterator {
    fn new(around: ChunkPosition, radius: i64) -> Self {
        ChunkIterator {
            radius,
            around,
            next_pos: Some(ChunkPosition(I64Vec3::new(-radius, -radius, -radius))),
        }
    }
}

impl Iterator for ChunkIterator {
    type Item = ChunkPosition;

    fn next(&mut self) -> Option<Self::Item> {
        let output = self.next_pos.take();
        if let Some(k) = output.as_ref() {
            self.next_pos = if k.x < self.radius {
                Some(ChunkPosition(I64Vec3::new(k.x + 1, k.y, k.z)))
            } else if k.y < self.radius {
                Some(ChunkPosition(I64Vec3::new(-self.radius, k.y + 1, k.z)))
            } else if k.z < self.radius {
                Some(ChunkPosition(I64Vec3::new(
                    -self.radius,
                    -self.radius,
                    k.z + 1,
                )))
            } else {
                None
            };
        }
        output.map(|a| ChunkPosition(self.around.0 + a.0))
    }
}
