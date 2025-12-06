mod block_breaking;
pub mod raycasting;

use crate::AppState;
use bevy::prelude::*;
pub use block_breaking::*;

/// Plugin that handles all block interactions (breaking, placing, etc.)
pub struct BlockInteractionPlugin;

impl Plugin for BlockInteractionPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<BlockBreakMessage>().add_systems(
            Update,
            (handle_block_breaking_input, apply_block_breaking)
                .chain()
                .run_if(in_state(AppState::InGame)),
        );
    }
}
