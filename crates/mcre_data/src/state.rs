use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockState {
    pub id: u16,
    pub block_id: u16,
    pub light_emission: u8,
    pub use_shape_for_light_occlusion: bool,
    pub propagates_skylight_down: bool,
    pub light_dampening: u8,
    pub solid_render: bool,
    pub is_randomly_ticking: bool,
    /// 6-bit mask of directions for which `state.skipRendering(state, dir)` is
    /// true (bit `n` = `Direction::index() == n`).
    #[serde(default)]
    pub self_skip: u8,
    /// 6-bit mask of directions for which `state.isFaceSturdy(...)` is true
    /// (bit `n` = `Direction::index() == n`).
    #[serde(default)]
    pub face_sturdy: u8,
    /// `state.isCollisionShapeFullBlock(EmptyBlockGetter.INSTANCE, BlockPos.ZERO)`
    #[serde(default)]
    pub collision_full: bool,
    /// `state.getShadeBrightness(EmptyBlockGetter.INSTANCE, BlockPos.ZERO)`
    #[serde(default = "default_shade_brightness")]
    pub shade_brightness: f32,
    /// Legacy `state.isSolid()` (`BlockBehaviour.BlockStateBase.legacySolid`),
    /// consumed by the fluid corner-height averaging.
    #[serde(default)]
    pub solid: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fluid: Option<String>,
    #[serde(default)]
    pub occlusion_shape: u16,
    #[serde(default)]
    pub emissive_rendering: bool,
    pub state_values: IndexMap<String, StateValue>,
}

fn default_shade_brightness() -> f32 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum StateValue {
    Bool(bool),
    Int(u8),
    String(String),
}

impl BlockState {
    pub fn all_sync() -> io::Result<Vec<Self>> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let block_state_data_path = root.join("block_states.json");
        let block_state_data_json = std::fs::read_to_string(block_state_data_path)?;
        Ok(serde_json::from_str(&block_state_data_json)?)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn all() -> io::Result<Vec<Self>> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let block_state_data_path = root.join("block_states.json");
        let block_state_data_json = fs::read_to_string(block_state_data_path).await?;
        let block_data: Vec<Self> = serde_json::from_str(&block_state_data_json)?;

        Ok(block_data)
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use crate::state::BlockState;

    #[tokio::test]
    async fn test_block_state_data_load() {
        let block_states = BlockState::all().await.unwrap();
        assert!(!block_states.is_empty());
    }
}
