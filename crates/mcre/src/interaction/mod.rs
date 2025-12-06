mod block_breaking;
mod block_placing;
pub mod raycasting;

use crate::AppState;
use bevy::prelude::*;
pub use block_breaking::*;
pub use block_placing::*;

/// Plugin that handles block interactions (breaking, placing, etc...)
pub struct BlockInteractionPlugin;

impl Plugin for BlockInteractionPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<BlockBreakMessage>()
            .add_systems(
                Update,
                (handle_block_breaking_input, apply_block_breaking)
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            )
            .add_message::<BlockPlaceMessage>()
            .add_systems(
                Update,
                (handle_block_placing_input, apply_block_placing)
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            );
    }
}
