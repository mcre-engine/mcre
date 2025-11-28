use serde::{Deserialize, Serialize};

use crate::BlockPos;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum OffsetType {
    None,
    XZ,
    XYZ,
}

impl OffsetType {
    #[inline]
    fn extract(seed: i64, shift: u32, scale: f32, base: f32) -> f32 {
        let bits = ((seed >> shift) & 15) as f32 / 15.0;
        (bits - base) * scale
    }

    #[inline]
    pub fn offset(
        &self,
        mut pos: BlockPos,
        max_horizontal_offset: f32,
        max_vertical_offset: f32,
    ) -> (f64, f64, f64) {
        match self {
            Self::None => (0.0, 0.0, 0.0),
            Self::XZ => {
                pos.y = 0;
                let seed = pos.seed();
                let x = Self::extract(seed, 0, 0.5, 0.5) as f64;
                let z = Self::extract(seed, 8, 0.5, 0.5) as f64;
                (
                    x.clamp(-max_horizontal_offset as f64, max_horizontal_offset as f64),
                    0.0,
                    z.clamp(-max_horizontal_offset as f64, max_horizontal_offset as f64),
                )
            }
            Self::XYZ => {
                pos.y = 0;
                let seed = pos.seed();
                let x = Self::extract(seed, 0, 0.5, 0.5) as f64;
                let y = Self::extract(seed, 4, max_vertical_offset, 1.0);
                let z = Self::extract(seed, 8, 0.5, 0.5) as f64;
                (
                    x.clamp(-max_horizontal_offset as f64, max_horizontal_offset as f64),
                    y as f64,
                    z.clamp(-max_horizontal_offset as f64, max_horizontal_offset as f64),
                )
            }
        }
    }
}
