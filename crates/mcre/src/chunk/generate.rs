use bevy::math::U8Vec2;
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
            chunk.set((x as u8, 3, y as u8), Block::DIRT);
            chunk.set((x as u8, 2, y as u8), Block::DIRT);
            chunk.set((x as u8, 1, y as u8), Block::DIRT);
            chunk.set((x as u8, 0, y as u8), Block::BEDROCK);
        }
    }

    let loc = U8Vec2::new(4, 4);
    for y in 4..10 {
        chunk.set((loc.x, y, loc.y), Block::OAK_LOG);
    }
    for x in 1..8 {
        for z in 1..8 {
            if x == z && x == loc.x {
                continue;
            }
            chunk.set((x, 7, z), Block::OAK_LEAVES);
        }
    }

    for x in 2..7 {
        for z in 2..7 {
            if x == z && x == loc.x {
                continue;
            }
            chunk.set((x, 8, z), Block::OAK_LEAVES);
        }
    }

    for x in 3..6 {
        for z in 3..6 {
            if x == z && x == loc.x {
                continue;
            }
            chunk.set((x, 9, z), Block::OAK_LEAVES);
        }
    }

    for x in 3..6 {
        for z in 3..6 {
            chunk.set((x, 10, z), Block::OAK_LEAVES);
        }
    }

    chunk.set((0, 11, 0), Block::DIAMOND_ORE);
    chunk.set(
        (
            chunk.chunk_size.as_u8() - 1,
            11,
            chunk.chunk_size.as_u8() - 1,
        ),
        Block::IRON_ORE,
    );
    chunk
}
