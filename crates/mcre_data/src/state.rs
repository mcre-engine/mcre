use indexmap::IndexMap;
use mcre_core::{BlockPos, OffsetType};
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum StateValue {
    Bool(bool),
    Int(u8),
    String(String),
}

impl BlockState {
    pub fn random_offset(&self, pos: BlockPos) -> (f64, f64, f64) {
        self.offset_type
            .offset(pos, self.max_horizontal_offset, self.max_vertical_offset)
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
    use jni::{JNIEnv, objects::JValueGen};
    use mcre_core::BlockPos;

    use crate::state::{BlockState, OffsetType};

    #[tokio::test]
    async fn test_block_state_data_load() {
        let block_states = BlockState::all().await.unwrap();
        assert!(!block_states.is_empty());
    }

    #[mcje::test]
    async fn test_random_offset(env: &mut JNIEnv<'_>) {
        let block_states = BlockState::all().await.unwrap();

        let block_state_registry = env
            .get_static_field(
                "net/minecraft/world/level/block/Block",
                "BLOCK_STATE_REGISTRY",
                "Lnet/minecraft/core/IdMapper;",
            )
            .unwrap()
            .l()
            .unwrap();

        let mut block_state = env
            .call_method(
                &block_state_registry,
                "byId",
                "(I)Ljava/lang/Object;",
                &[JValueGen::Int(0)],
            )
            .unwrap()
            .l()
            .unwrap();

        let mut block_state_id = 0u16;

        while !block_state.is_null() {
            let offset_function = env
                .get_field(
                    &block_state,
                    "offsetFunction",
                    "Lnet/minecraft/world/level/block/state/BlockBehaviour$OffsetFunction;",
                )
                .unwrap()
                .l()
                .unwrap();
            if offset_function.is_null() {
                assert_eq!(
                    block_states[block_state_id as usize].offset_type,
                    OffsetType::None
                );
            } else {
                let block_pos_class = "net/minecraft/core/BlockPos";

                for i in 0..10 {
                    // BlockPos(i, i, i)
                    let pos_obj = env
                        .new_object(
                            block_pos_class,
                            "(III)V",
                            &[JValueGen::Int(i), JValueGen::Int(i), JValueGen::Int(i)],
                        )
                        .unwrap();

                    // Call offsetFunction.evaluate(state, pos)
                    // Signature: (LBlockState;LBlockPos;)LVec3;
                    let vec3_obj = env.call_method(
                        &offset_function,
                        "evaluate",
                        "(Lnet/minecraft/world/level/block/state/BlockState;Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/phys/Vec3;",
                        &[JValueGen::Object(&block_state), JValueGen::Object(&pos_obj)]
                    ).unwrap().l().unwrap();

                    let java_x = env.get_field(&vec3_obj, "x", "D").unwrap().d().unwrap();
                    let java_y = env.get_field(&vec3_obj, "y", "D").unwrap().d().unwrap();
                    let java_z = env.get_field(&vec3_obj, "z", "D").unwrap().d().unwrap();

                    let java_value = (java_x, java_y, java_z);

                    let block_state = &block_states[block_state_id as usize];

                    let rust_value = block_state.random_offset(BlockPos::new(i, i, i));

                    assert!(
                        (rust_value.0 - java_value.0).abs() < 1e-6,
                        "State: {:#?}\nBlockPos({i}, {i}, {i})\nOffset:\n  Rust: ({}, {}, {})\n  Java: ({}, {}, {})",
                        block_state,
                        rust_value.0,
                        rust_value.1,
                        rust_value.2,
                        java_value.0,
                        java_value.1,
                        java_value.2
                    );

                    assert!(
                        (rust_value.1 - java_value.1).abs() < 1e-6,
                        "State: {:#?}\nBlockPos({i}, {i}, {i})\nOffset:\n  Rust: ({}, {}, {})\n  Java: ({}, {}, {})",
                        block_state,
                        rust_value.0,
                        rust_value.1,
                        rust_value.2,
                        java_value.0,
                        java_value.1,
                        java_value.2
                    );

                    assert!(
                        (rust_value.2 - java_value.2).abs() < 1e-6,
                        "State: {:#?}\nBlockPos({i}, {i}, {i})\nOffset:\n  Rust: ({}, {}, {})\n  Java: ({}, {}, {})",
                        block_state,
                        rust_value.0,
                        rust_value.1,
                        rust_value.2,
                        java_value.0,
                        java_value.1,
                        java_value.2
                    );
                }
            }
            block_state_id += 1;
            block_state = env
                .call_method(
                    &block_state_registry,
                    "byId",
                    "(I)Ljava/lang/Object;",
                    &[JValueGen::Int(block_state_id.into())],
                )
                .unwrap()
                .l()
                .unwrap();
        }
    }
}
