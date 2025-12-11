pub mod rng;

use bevy::{math::U8Vec2, prelude::*};
use mcre_core::Block;

use crate::chunk::{
    Chunk,
    generate::rng::{ChunkRng, FractalNoiseSettings},
    math::{pos::ChunkPosition, size::ChunkSize},
};

pub fn generate_chunk(chunk_size: ChunkSize, loc: ChunkPosition, rng: &ChunkRng) -> Chunk {
    let mut chunk = Chunk::empty(chunk_size, loc);
    for x in chunk.size().iter() {
        for z in chunk.size().iter() {
            chunk.set((x, -64, z), Block::BEDROCK);
        }
    }

    let world = loc.world_coord(chunk_size);
    let world = Vec2::new(world.x, world.z);

    let settings = FractalNoiseSettings {
        freq: 0.007,
        amplitude: 20.0,
        octaves: 4,
        persistence: 0.5,
    };
    for x in chunk.size().iter() {
        for z in chunk.size().iter() {
            for y in -63..50 {
                let cur = world + Vec2::new(x as f32, z as f32);
                let surface_y = rng.fractal_noise(cur, settings);
                if (y as f64) < surface_y {
                    chunk.set((x, y, z), Block::STONE);
                }
            }
        }
    }

    chunk
}

#[allow(unused)]
pub fn spawn_test_chunk(chunk_size: ChunkSize, loc: ChunkPosition) -> Chunk {
    let mut chunk = Chunk::empty(chunk_size, loc);

    for x in chunk.size().iter() {
        for y in chunk.size().iter() {
            chunk.set((x, 3, y), Block::DIRT);
            chunk.set((x, 2, y), Block::DIRT);
            chunk.set((x, 1, y), Block::DIRT);
            chunk.set((x, 0, y), Block::BEDROCK);
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
