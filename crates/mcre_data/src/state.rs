use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::PathBuf;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockState {
    pub id: u16,
    pub block_id: u16,
    pub block_name: String,
    pub light_emission: u8,
    pub use_shape_for_light_occlusion: bool,
    pub propagates_skylight_down: bool,
    pub light_dampening: u8,
    pub solid_render: bool,
    pub is_randomly_ticking: bool,
    pub state_values: IndexMap<String, StateValue>,
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
