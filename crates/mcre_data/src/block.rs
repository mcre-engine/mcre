use mcre_core::OffsetType;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::PathBuf;
use tokio::fs;

#[derive(Debug, Serialize, Deserialize)]
pub struct Block {
    pub id: u16,
    pub name: String,         // "oak_planks"
    pub display_name: String, // "Oak Planks",
    pub default_state_id: u16,
    pub min_state_id: u16,
    pub max_state_id: u16,
    pub states: Vec<BlockStateField>,
    pub has_collision: bool,
    pub explosion_resistance: f32,
    pub friction: f32,
    pub speed_factor: f32,
    pub jump_factor: f32,
    pub bounce_restitution: f32,
    pub fall_distance_reduction: f32,
    pub push_reaction: String,
    pub instrument: String,
    pub destroy_speed: f32,
    pub requires_correct_tool_for_drops: bool,
    pub can_occlude: bool,
    pub ignited_by_lava: bool,
    pub is_air: bool,
    pub spawn_terrain_particles: bool,
    pub replaceable: bool,
    pub offset_type: OffsetType,
    pub max_horizontal_offset: f32,
    pub max_vertical_offset: f32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct BlockStateField {
    pub name: String,
    #[serde(flatten)]
    pub values: BlockStateFieldValues,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum BlockStateFieldValues {
    Bool,
    Enum {
        enum_name: String,
        values: Vec<String>,
    },
    Int {
        // inclusive range, [min, max]
        min: u8,
        max: u8,
    },
}

impl BlockStateFieldValues {
    pub fn is_bool(&self) -> bool {
        matches!(self, Self::Bool)
    }

    pub fn is_enum(&self) -> bool {
        matches!(self, Self::Enum { .. })
    }

    pub fn is_int(&self) -> bool {
        matches!(self, Self::Int { .. })
    }

    pub fn as_enum(&self) -> Option<(&str, &[String])> {
        if let Self::Enum { enum_name, values } = self {
            Some((enum_name, values))
        } else {
            None
        }
    }

    pub fn as_int(&self) -> Option<(u8, u8)> {
        if let Self::Int { min, max } = self {
            Some((*min, *max))
        } else {
            None
        }
    }
}

impl Block {
    pub fn all_sync() -> io::Result<Box<[Self]>> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let block_data_path = root.join("blocks.json");
        let block_data_json = std::fs::read_to_string(block_data_path)?;
        Ok(serde_json::from_str(&block_data_json)?)
    }

    pub async fn all() -> io::Result<Vec<Self>> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let block_data_path = root.join("blocks.json");
        let block_data_json = fs::read_to_string(block_data_path).await?;
        let block_data: Vec<Self> = serde_json::from_str(&block_data_json)?;

        Ok(block_data)
    }
}

#[cfg(test)]
mod tests {
    use crate::block::Block;

    #[tokio::test]
    async fn test_block_data_load() {
        let blocks = Block::all().await.unwrap();
        assert!(!blocks.is_empty());
    }
}
