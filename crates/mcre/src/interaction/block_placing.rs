use crate::chunk::{Chunk, world_pos_to_chunk_pos};
use crate::chunk_map::ChunkMap;
use crate::interaction::raycasting::{BlockRaycastHit, raycast_block_data};
use crate::textures::BlockTextures;
use bevy::prelude::*;
use mcre_core::{Block, Direction};

#[derive(Message, Clone, Event)]
pub struct BlockPlaceMessage(pub BlockRaycastHit);

pub fn handle_block_placing_input(
    mouse_input: Res<ButtonInput<MouseButton>>,
    camera_query: Query<&Transform, With<Camera>>,
    chunk_map: Res<ChunkMap>,
    chunks_query: Query<&Chunk>,
    mut place_event_writer: MessageWriter<BlockPlaceMessage>,
) {
    let Ok(camera_transform) = camera_query.single() else {
        return;
    };

    if !mouse_input.just_pressed(MouseButton::Right) {
        return;
    }

    let ray_origin = camera_transform.translation;
    let ray_direction = camera_transform.forward();

    if let Some(hit) = raycast_block_data(ray_origin, *ray_direction, &chunk_map, &chunks_query) {
        place_event_writer.write(BlockPlaceMessage(hit));
    }
}

pub fn apply_block_placing(
    mut events: MessageReader<BlockPlaceMessage>,
    chunk_map: Res<ChunkMap>,
    mut chunks_query: Query<(&mut Chunk, &mut Mesh3d)>,
    textures: Res<BlockTextures>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for event in events.read() {
        let hit: &BlockRaycastHit = &event.0;

        let place_world_pos = match hit.face {
            Direction::North => hit.block_pos + IVec3::new(0, 0, -1),
            Direction::South => hit.block_pos + IVec3::new(0, 0, 1),
            Direction::East => hit.block_pos + IVec3::new(1, 0, 0),
            Direction::West => hit.block_pos + IVec3::new(-1, 0, 0),
            Direction::Up => hit.block_pos + IVec3::new(0, 1, 0),
            Direction::Down => hit.block_pos + IVec3::new(0, -1, 0),
        };

        let (chunk_world_pos, local_pos) = world_pos_to_chunk_pos(place_world_pos);

        let Some(&chunk_entity) = chunk_map.0.get(&chunk_world_pos) else {
            continue;
        };

        if let Ok((mut chunk, mut mesh_handle)) = chunks_query.get_mut(chunk_entity) {
            if let Some(block_state) = chunk.get(local_pos) {
                if block_state.block() == Block::AIR {
                    chunk.set_block(local_pos, Block::DIRT);
                    mesh_handle.0 = chunk.regenerate_mesh(&textures, &mut meshes);
                    // info!(
                    //     "Placed block at world {:?} (chunk {:?}, local {:?})",
                    //     place_world_pos, chunk_world_pos, local_pos
                    // );
                }
            }
        }
    }
}
