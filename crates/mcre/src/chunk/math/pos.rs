use std::ops::{Add, Deref};

use bevy::math::{BVec2, I64Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::chunk::math::size::ChunkSize;

/// Chunk's position in world (or relative world) coordinates
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Deserialize, Serialize)]
pub struct ChunkPosition(I64Vec2);

impl ChunkPosition {
    /// Yields the [ChunkPosition] from the world coordinates based on the chunk size
    pub fn from_world_coord(world_coord: Vec3, size: ChunkSize) -> Self {
        let chunk = (world_coord / size.as_f32()).floor();
        ChunkPosition(I64Vec2::new(chunk.x as i64, chunk.z as i64))
    }

    /// Yields the world coordinates from the chunk size of this [ChunkPosition]
    pub fn into_world_coord(self, size: ChunkSize) -> Vec3 {
        let v = self.0.as_vec2();
        size.as_vec() * Vec3::new(v.x, 0., v.y)
    }

    pub fn iter_around(self, radius: u64) -> impl Iterator<Item = Self> {
        ChunkIterator::new(self, radius as i64)
    }

    pub fn outside_radius(self, other: Self, radius: u64) -> bool {
        self.x.abs_diff(other.x) > radius || self.y.abs_diff(other.y) > radius
    }
}

impl Deref for ChunkPosition {
    type Target = I64Vec2;

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
            next_pos: Some(ChunkPosition(I64Vec2::new(-radius, -radius))),
        }
    }
}

impl Iterator for ChunkIterator {
    type Item = ChunkPosition;

    fn next(&mut self) -> Option<Self::Item> {
        let output = self.next_pos.take();
        if let Some(k) = output.as_ref() {
            self.next_pos = if k.x < self.radius {
                Some(ChunkPosition(I64Vec2::new(k.x + 1, k.y)))
            } else if k.y < self.radius {
                Some(ChunkPosition(I64Vec2::new(-self.radius, k.y + 1)))
            } else {
                None
            };
        }
        output.map(|a| ChunkPosition(self.around.0 + a.0))
    }
}

/// Defines a relative block position within a chunk
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlockPosition {
    pub x: u8,
    pub y: i64,
    pub z: u8,
}

impl BlockPosition {
    pub const fn in_bounds(self, size: ChunkSize) -> BVec2 {
        let size = size.as_usize();
        BVec2::new((self.x as usize) < size, (self.z as usize) < size)
    }

    pub fn into_world_coord(self, pos: ChunkPosition, size: ChunkSize) -> Vec3 {
        let chunk_world = pos.into_world_coord(size);
        chunk_world + Vec3::new(self.x as f32, self.y as f32, self.z as f32)
    }

    // -Z is North, +Z is South
    // -X is West, +X is East

    pub const fn north(mut self) -> Self {
        self.z -= 1;
        self
    }

    pub const fn south(mut self) -> Self {
        self.z += 1;
        self
    }

    pub const fn west(mut self) -> Self {
        self.x -= 1;
        self
    }

    pub const fn east(mut self) -> Self {
        self.x += 1;
        self
    }

    pub const fn down(mut self) -> Self {
        self.y -= 1;
        self
    }

    pub const fn up(mut self) -> Self {
        self.y += 1;
        self
    }
}

impl From<(u8, i64, u8)> for BlockPosition {
    fn from((x, y, z): (u8, i64, u8)) -> Self {
        BlockPosition { x, y, z }
    }
}

impl Add<u8> for BlockPosition {
    type Output = BlockPosition;

    fn add(self, rhs: u8) -> Self::Output {
        BlockPosition {
            x: self.x + rhs,
            y: self.y + rhs as i64,
            z: self.z + rhs,
        }
    }
}

impl PartialEq<(u8, i64, u8)> for BlockPosition {
    fn eq(&self, other: &(u8, i64, u8)) -> bool {
        self.x == other.0 && self.y == other.1 && self.z == other.2
    }
}

#[cfg(test)]
mod tests {
    use crate::chunk::math::pos::BlockPosition;

    #[test]
    fn test_direction() {
        let pos = BlockPosition { x: 0, y: 3, z: 0 };

        assert_eq!(pos.south(), BlockPosition { x: 0, y: 3, z: 1 });
    }
}
