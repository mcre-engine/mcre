use crate::chunk::{Chunk, world_pos_to_chunk_pos};
use crate::chunk_map::ChunkMap;
use bevy::prelude::*;
use mcre_core::{Block, BlockState, Direction};

pub const MAX_REACH_DISTANCE: f32 = 5.0;

#[derive(Debug, Clone, Copy)]
pub struct BlockRaycastHit {
    pub block_pos: IVec3,
    pub chunk_local_pos: UVec3,
    #[allow(unused)] // todo maybe remvoe later if not needed
    pub chunk_world_pos: IVec3,
    pub chunk_entity: Entity,
    pub distance: f32,
    pub face: Direction,
    pub block: Block,
    #[allow(unused)] // todo maybe remvoe later if not needed
    pub block_state: BlockState,
}

pub fn raycast_block_data(
    origin: Vec3,
    direction: Vec3,
    chunk_map: &ChunkMap,
    chunk_query: &Query<&Chunk>,
) -> Option<BlockRaycastHit> {
    let direction = direction.normalize();

    let step = Vec3::new(
        if direction.x > 0.0 { 1.0 } else { -1.0 },
        if direction.y > 0.0 { 1.0 } else { -1.0 },
        if direction.z > 0.0 { 1.0 } else { -1.0 },
    );

    let delta = Vec3::new(
        (1.0 / direction.x).abs(),
        (1.0 / direction.y).abs(),
        (1.0 / direction.z).abs(),
    );

    let mut block_pos = origin.floor().as_ivec3();

    let mut t_max = Vec3::new(
        if direction.x > 0.0 {
            (block_pos.x as f32 + 1.0 - origin.x) / direction.x
        } else {
            (origin.x - block_pos.x as f32) / -direction.x
        },
        if direction.y > 0.0 {
            (block_pos.y as f32 + 1.0 - origin.y) / direction.y
        } else {
            (origin.y - block_pos.y as f32) / -direction.y
        },
        if direction.z > 0.0 {
            (block_pos.z as f32 + 1.0 - origin.z) / direction.z
        } else {
            (origin.z - block_pos.z as f32) / -direction.z
        },
    );

    let mut distance = 0.0;
    let mut face = Direction::North;

    while distance < MAX_REACH_DISTANCE {
        if let Some((chunk_local_pos, chunk_world_pos, chunk_entity, block_state)) =
            check_block_at_position_data(block_pos, chunk_map, chunk_query)
        {
            return Some(BlockRaycastHit {
                block_pos,
                chunk_local_pos,
                chunk_world_pos,
                chunk_entity,
                distance,
                face,
                block: block_state.block(),
                block_state,
            });
        }

        if t_max.x < t_max.y && t_max.x < t_max.z {
            block_pos.x += step.x as i32;
            distance = t_max.x;
            t_max.x += delta.x;
            face = if step.x > 0.0 {
                Direction::West
            } else {
                Direction::East
            };
        } else if t_max.y < t_max.z {
            block_pos.y += step.y as i32;
            distance = t_max.y;
            t_max.y += delta.y;
            face = if step.y > 0.0 {
                Direction::Down
            } else {
                Direction::Up
            };
        } else {
            block_pos.z += step.z as i32;
            distance = t_max.z;
            t_max.z += delta.z;
            face = if step.z > 0.0 {
                Direction::North
            } else {
                Direction::South
            };
        }
    }

    None
}

fn check_block_at_position_data(
    world_pos: IVec3,
    chunk_map: &ChunkMap,
    chunk_query: &Query<&Chunk>,
) -> Option<(UVec3, IVec3, Entity, BlockState)> {
    let (chunk_world_pos, chunk_local_pos) = world_pos_to_chunk_pos(world_pos);

    if let Some(&entity) = chunk_map.0.get(&chunk_world_pos) {
        if let Ok(chunk) = chunk_query.get(entity) {
            if let Some(block_state) = chunk.get(chunk_local_pos) {
                if !block_state.is_air() {
                    return Some((chunk_local_pos, chunk_world_pos, entity, *block_state));
                }
            }
        }
    }
    None
}
