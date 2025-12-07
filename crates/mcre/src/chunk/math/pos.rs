use std::ops::{Add, Deref};

use bevy::math::{BVec2, I64Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::chunk::math::size::ChunkSize;

/// Chunk's position in world (or relative world) coordinates
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Deserialize, Serialize)]
pub struct ChunkPosition(I64Vec2);

impl ChunkPosition {
    pub fn into_coords(pos: Vec3) -> Self {
        let chunk = pos.floor();
        ChunkPosition(I64Vec2::new(chunk.x as i64, chunk.z as i64))
    }

    pub fn world_coord(self, size: ChunkSize) -> Vec3 {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlockPosition {
    pub x: u8,
    pub y: i64,
    pub z: u8,
}

impl BlockPosition {
    /// Index of block inside the chunk array based off this size
    /// These are local coordinates to the chunk
    pub const fn to_index(self, size: ChunkSize) -> usize {
        let mut y = self.y.abs() * 2;
        let y_sign = self.y.signum();
        y -= (y_sign.abs() - y_sign) / 2;

        let size = size.as_usize();
        y as usize * size.pow(2) + self.x as usize * size + self.z as usize
    }

    /// Constructs the x,y,z position from the index in the chunk array
    pub fn from_index(mut index: usize, size: ChunkSize) -> Self {
        let size = size.as_usize();
        let d = size.pow(2);
        let y = index / d;
        let j = y / 2;
        let k = (y % 2) as i64;
        let y = j as i64 * (1 - 2 * k);
        index %= d;

        let x = (index / size) as u8;
        index %= size;

        let z = index as u8;
        BlockPosition { x, y, z }
    }

    pub const fn in_bounds(self, size: ChunkSize) -> BVec2 {
        let size = size.as_usize();
        BVec2::new((self.x as usize) < size, (self.z as usize) < size)
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

#[cfg(test)]
mod tests {
    use crate::chunk::math::{pos::BlockPosition, size::ChunkSize};

    #[test]
    fn test_to_index() {
        let size = ChunkSize::new(16);
        let pos = BlockPosition { x: 0, y: 3, z: 0 };

        let idx = pos.to_index(size);
        assert_eq!(idx, 6 * 16 * 16);
        assert_eq!(BlockPosition::from_index(idx, size), pos);

        let pos = BlockPosition { x: 0, y: -10, z: 0 };
        let idx = pos.to_index(size);
        assert_eq!(idx, 19 * 16 * 16);

        let pos = BlockPosition { x: 0, y: 10, z: 0 };
        let idx = pos.to_index(size);
        assert_eq!(idx, 20 * 16 * 16);
        assert_eq!(pos, BlockPosition::from_index(idx, size));

        let pos = BlockPosition { x: 4, y: 8, z: 4 };
        let idx = pos.to_index(size);
        assert_eq!(pos, BlockPosition::from_index(idx, size));
    }

    #[test]
    fn test_direction() {
        let pos = BlockPosition { x: 0, y: 3, z: 0 };

        assert_eq!(pos.south(), BlockPosition { x: 0, y: 3, z: 1 });
    }
}
