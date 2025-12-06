use crate::{
    chunk::Chunk,
    chunk_map::ChunkMap,
    interaction::raycasting::{BlockRaycastHit, raycast_block_data},
};
use bevy::prelude::*;

#[derive(Component)]
pub struct TargetedBlockText;

impl TargetedBlockText {
    pub fn into_bundle(self) -> impl Bundle {
        (
            self,
            Text::new("Looking at: None"),
            TextFont {
                font_size: 20.0,
                ..Default::default()
            },
            TextColor(Color::WHITE),
        )
    }

    fn format_text(hit: Option<BlockRaycastHit>) -> String {
        if let Some(hit) = hit {
            format!(
                "Looking at: {} at ({}, {}, {})\nDistance: {:.2} blocks\nFace: {:?}",
                hit.block.display_name(),
                hit.block_pos.x,
                hit.block_pos.y,
                hit.block_pos.z,
                hit.distance,
                hit.face
            )
        } else {
            "Looking at: None".to_string()
        }
    }

    pub fn update_text_system(
        mut ui_query: Query<&mut Text, With<TargetedBlockText>>,
        camera_query: Query<&Transform, With<Camera>>,
        chunk_map: Res<ChunkMap>,
        chunks_query: Query<&Chunk>,
    ) {
        let Ok(mut ui) = ui_query.single_mut() else {
            return;
        };

        let Ok(camera_transform) = camera_query.single() else {
            return;
        };

        // Perform raycast from camera
        let ray_origin = camera_transform.translation;
        let ray_direction = camera_transform.forward();

        let hit = raycast_block_data(ray_origin, *ray_direction, &chunk_map, &chunks_query);
        ui.0 = Self::format_text(hit);
    }
}
