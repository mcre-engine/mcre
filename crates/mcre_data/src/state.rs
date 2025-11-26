use indexmap::IndexMap;
use mcre_core::{BlockPos, StateValue};
use serde::{Deserialize, Serialize};
use std::io;
use std::path::PathBuf;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockState {
    pub id: u16,
    pub block_id: u16,
    pub block_name: String,
    // Light / Rendering
    // How much light the block emits (0–15).
    pub light_emission: u8,
    // If true, use the actual voxel shape for light occlusion (glass panes, fences).
    pub use_shape_for_light_occlusion: bool,
    // True if skylight passes through this block.
    pub propagates_skylight_down: bool,
    // How many light levels this block blocks (0–15).
    // Opposite of emission.
    pub light_block: u8,
    // Whether it is considered solid for rendering (solid faces culling).
    pub solid_render: bool,
    // Block Solidity + Physics
    // If this blockstate is "air" (no collision, no rendering, etc).
    pub is_air: bool,
    // Whether lava should ignite this block.
    pub ignited_by_lava: bool,
    // Whether this block can occlude other blocks (block light / face culling).
    pub can_occlude: bool,
    // TODO:
    // pub map_color: MapColor,
    pub is_randomly_ticking: bool,
    pub replaceable: bool,
    pub spawn_terrain_particles: bool,
    pub requires_correct_tool_for_drops: bool,
    pub destroy_speed: f32,
    pub offset_type: OffsetType,
    pub max_horizontal_offset: f32,
    pub max_vertical_offset: f32,
    pub state_values: IndexMap<String, StateValue>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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
    pub fn offset(&self, pos: BlockPos) -> (f32, f32, f32) {
        match self {
            Self::None => (0.0, 0.0, 0.0),
            Self::XZ => {
                let seed = pos.seed();
                let x = Self::extract(seed, 16, 0.5, 0.5);
                let z = Self::extract(seed, 24, 0.5, 0.5);
                (x, 0.0, z)
            }
            Self::XYZ => {
                let seed = pos.seed();
                let x = Self::extract(seed, 16, 0.5, 0.5);
                let y = Self::extract(seed, 20, 0.2, 1.0);
                let z = Self::extract(seed, 24, 0.5, 0.5);
                (x, y, z)
            }
        }
    }
}

impl BlockState {
    pub async fn all() -> io::Result<Vec<Self>> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let block_state_data_path = root.join("block_states.json");
        let block_state_data_json = fs::read_to_string(block_state_data_path).await?;
        let block_data: Vec<Self> = serde_json::from_str(&block_state_data_json)?;

        Ok(block_data)
    }
}

#[cfg(test)]
mod tests {
    use crate::state::BlockState;

    #[tokio::test]
    async fn test_block_state_data_load() {
        let block_states = BlockState::all().await.unwrap();
        assert!(!block_states.is_empty());
    }
}
