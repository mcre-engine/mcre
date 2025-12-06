use crate::chunk::Chunk;
use crate::chunk_map::ChunkMap;
use crate::interaction::raycasting::{BlockRaycastHit, raycast_block_data};
use crate::textures::BlockTextures;
use bevy::prelude::*;
use mcre_core::Block;

#[derive(Message, Clone, Event)]
pub struct BlockBreakMessage(pub BlockRaycastHit);

pub fn handle_block_breaking_input(
    mouse_input: Res<ButtonInput<MouseButton>>,
    camera_query: Query<&Transform, With<Camera>>,
    chunk_map: Res<ChunkMap>,
    chunks_query: Query<&Chunk>,
    mut break_event_writer: MessageWriter<BlockBreakMessage>,
) {
    let Ok(camera_transform) = camera_query.single() else {
        return;
    };

    if !mouse_input.just_pressed(MouseButton::Left) {
        return;
    }

    let ray_origin = camera_transform.translation;
    let ray_direction = camera_transform.forward();

    // Perform raycast using the efficient ChunkMap
    if let Some(hit) = raycast_block_data(ray_origin, *ray_direction, &chunk_map, &chunks_query) {
        break_event_writer.write(BlockBreakMessage(hit));
    }
}

pub fn apply_block_breaking(
    mut events: MessageReader<BlockBreakMessage>,
    mut chunks_query: Query<(&mut Chunk, &mut Mesh3d)>,
    textures: Res<BlockTextures>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for event in events.read() {
        let hit = &event.0;
        if let Ok((mut chunk, mut mesh_handle)) = chunks_query.get_mut(hit.chunk_entity) {
            chunk.set_block(hit.chunk_local_pos, Block::AIR);

            mesh_handle.0 = chunk.regenerate_mesh(&textures, &mut meshes);

            // info!("Broke block {:?} at {:?}", hit.block, hit.block_pos);
        }
    }
}
