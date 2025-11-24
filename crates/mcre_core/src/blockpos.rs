use crate::Vec3i;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl BlockPos {
    pub const ZERO: Self = Self::new(0, 0, 0);

    #[inline]
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    #[inline]
    pub fn seed(self) -> i64 {
        let mut i = (self.x as i64).wrapping_mul(3_129_871)
            ^ (self.z as i64).wrapping_mul(116_129_781)
            ^ (self.y as i64);

        i = i
            .wrapping_mul(i)
            .wrapping_mul(42_317_861)
            .wrapping_add(i.wrapping_mul(11));

        i
    }
}

impl From<Vec3i> for BlockPos {
    #[inline]
    fn from(vec: Vec3i) -> Self {
        Self {
            x: vec[0],
            y: vec[1],
            z: vec[2],
        }
    }
}
