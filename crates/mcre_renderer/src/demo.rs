use mcre_world::{Block, Chunk, ChunkPos, World};

/// Camera used by the demo scene in both the native and browser viewers.
pub const DEMO_CAMERA_POSITION: [f32; 3] = [8.0, 7.0, 26.0];

/// A small flat world shared by the native and wasm viewers. Keeping the
/// scene in the library lets the browser smoke test compare its render
/// against a native capture of the same geometry.
pub fn demo_world() -> World {
    let mut world = World::new(1);
    for cx in -1..=1 {
        for cz in -1..=1 {
            let mut chunk = Chunk::new_at(ChunkPos::new(cx, cz));
            for x in 0..16 {
                for z in 0..16 {
                    chunk.set_block(x, 0, z, Block::STONE.default_state());
                    chunk.set_block(x, 1, z, Block::GRASS_BLOCK.default_state());
                }
            }
            world.add_chunk(chunk);
        }
    }
    world.compute_light();
    world
}
