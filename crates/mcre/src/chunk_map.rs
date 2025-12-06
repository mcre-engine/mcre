use crate::chunk::Chunk;
use bevy::prelude::*;
use std::collections::HashMap;

#[derive(Resource, Default, Debug)]
pub struct ChunkMap(pub HashMap<IVec3, Entity>);

pub fn update_chunk_map_system(
    mut chunk_map: ResMut<ChunkMap>,
    query: Query<(Entity, &Transform), Added<Chunk>>,
) {
    for (entity, transform) in query.iter() {
        let chunk_pos = transform.translation.as_ivec3();
        chunk_map.0.insert(chunk_pos, entity);
        // info!("Added chunk at {} to ChunkMap", chunk_pos);
    }
}
