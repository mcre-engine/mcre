use bevy::prelude::*;
use mcre_core::Block;

use crate::chunk::{
    Chunk,
    math::{pos::ChunkPosition, size::ChunkSize},
};

// TODO: Create procedural
pub fn spawn_test_chunk(chunk_size: ChunkSize, loc: ChunkPosition) -> Chunk {
    let mut chunk = Chunk::empty(chunk_size, loc);

    for x in chunk.size().iter() {
        for y in chunk.size().iter() {
            chunk.set_block(UVec3::new(x as u32, 3, y as u32), Block::DIRT);
            chunk.set_block(UVec3::new(x as u32, 2, y as u32), Block::DIRT);
            chunk.set_block(UVec3::new(x as u32, 1, y as u32), Block::DIRT);
            chunk.set_block(UVec3::new(x as u32, 0, y as u32), Block::BEDROCK);
        }
    }

    let loc = UVec2::new(4, 4);
    for y in 4..10 {
        chunk.set_block(UVec3::new(loc.x, y as u32, loc.y), Block::OAK_LOG);
    }
    for x in 1..8 {
        for z in 1..8 {
            if x == z && x == loc.x {
                continue;
            }
            chunk.set_block(UVec3::new(x, 7, z), Block::OAK_LEAVES);
        }
    }

    for x in 2..7 {
        for z in 2..7 {
            if x == z && x == loc.x {
                continue;
            }
            chunk.set_block(UVec3::new(x, 8, z), Block::OAK_LEAVES);
        }
    }

    for x in 3..6 {
        for z in 3..6 {
            if x == z && x == loc.x {
                continue;
            }
            chunk.set_block(UVec3::new(x, 9, z), Block::OAK_LEAVES);
        }
    }

    for x in 3..6 {
        for z in 3..6 {
            chunk.set_block(UVec3::new(x, 10, z), Block::OAK_LEAVES);
        }
    }

    chunk.set_block(UVec3::new(0, 11, 0), Block::DIAMOND_ORE);
    chunk.set_block(
        UVec3::new(
            chunk.chunk_size.as_u32() - 1,
            11,
            chunk.chunk_size.as_u32() - 1,
        ),
        Block::IRON_ORE,
    );
    chunk
}
