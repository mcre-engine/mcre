mod block_breaking;
pub mod raycasting;

pub use block_breaking::*;

use crate::AppState;
use bevy::prelude::*;

/// Plugin that handles all block interactions (breaking, placing, etc.)
pub struct BlockInteractionPlugin;

impl Plugin for BlockInteractionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            handle_block_breaking.run_if(in_state(AppState::InGame)),
        );
    }
}
