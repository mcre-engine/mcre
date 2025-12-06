use crate::chunk::Chunk;
use crate::interaction::raycasting::raycast_block_data;
use crate::textures::BlockTextures;
use bevy::prelude::*;
use mcre_core::Block;

/// System that handles breaking blocks when player left-clicks
pub fn handle_block_breaking(
    mouse_input: Res<ButtonInput<MouseButton>>,
    camera_query: Query<&Transform, With<Camera>>,
    mut chunks_query: Query<(Entity, &mut Chunk, &Transform)>,
    textures: Res<BlockTextures>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mesh_query: Query<&mut Mesh3d>,
) {
    // Only trigger on click press (not held)
    if !mouse_input.just_pressed(MouseButton::Left) {
        return;
    }

    let Ok(camera_transform) = camera_query.single() else {
        return;
    };

    // Perform raycast from camera
    let ray_origin = camera_transform.translation;
    let ray_direction = camera_transform.forward();

    // Collect chunk data for raycasting (avoids query conflicts)
    let chunks_data: Vec<_> = chunks_query
        .iter()
        .map(|(_, chunk, transform)| (chunk.clone(), transform.translation.as_ivec3()))
        .collect();

    // Perform raycast using the collected data
    let Some(hit) = raycast_block_data(ray_origin, ray_direction.into(), &chunks_data) else {
        return;
    };

    // Find and modify the chunk containing the hit block
    for (entity, mut chunk, transform) in chunks_query.iter_mut() {
        let chunk_world_pos = transform.translation.as_ivec3();

        if chunk_world_pos == hit.chunk_world_pos {
            chunk.set_block(hit.chunk_local_pos, Block::AIR);

            if let Ok(mut mesh_handle) = mesh_query.get_mut(entity) {
                *mesh_handle = Mesh3d(chunk.regenerate_mesh(&textures, &mut meshes));
            }

            info!("Broke block {:?} at {:?}", hit.block, hit.block_pos);
            break;
        }
    }
}
