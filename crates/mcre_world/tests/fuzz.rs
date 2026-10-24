use std::collections::HashMap;

use mcre_world::{Block, BlockState, Chunk, WORLD_HEIGHT, WORLD_MIN_Y, World};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn air() -> BlockState {
    Block::AIR.default_state()
}

fn pick_state(rng: &mut Rng) -> BlockState {
    if rng.below(2) == 0 {
        match rng.below(8) {
            0 => air(),
            1 => Block::STONE.default_state(),
            2 => Block::DIRT.default_state(),
            3 => Block::SAND.default_state(),
            4 => Block::OAK_LOG.default_state(),
            5 => Block::WATER.default_state(),
            6 => Block::GLASS.default_state(),
            _ => Block::COBBLESTONE.default_state(),
        }
    } else {
        BlockState::from(1000 + rng.below(400) as u16)
    }
}

fn pick_y(rng: &mut Rng) -> i32 {
    if rng.below(2) == 0 {
        WORLD_MIN_Y + rng.below(16) as i32
    } else {
        WORLD_MIN_Y + rng.below(WORLD_HEIGHT as u64) as i32
    }
}

#[test]
fn fuzz_chunk_across_optimize() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut chunk = Chunk::new();
    let mut reference: HashMap<(i32, i32, i32), BlockState> = HashMap::new();

    for op in 0..200_000 {
        let x = rng.below(16) as i32;
        let z = rng.below(16) as i32;
        let y = pick_y(&mut rng);
        let state = pick_state(&mut rng);

        chunk.set_block(x, y, z, state);
        reference.insert((x, y, z), state);

        if op % 5_000 == 4_999 {
            chunk.optimize();
            for (&(x, y, z), &expected) in &reference {
                assert_eq!(chunk.get_block(x, y, z), expected, "pos ({x}, {y}, {z})");
            }
        }
    }

    let iterated: HashMap<_, _> = chunk
        .iter_blocks()
        .map(|(x, y, z, state)| ((x, y, z), state))
        .collect();
    let expected: HashMap<_, _> = reference
        .into_iter()
        .filter(|(_, state)| !state.is_air())
        .collect();
    assert_eq!(iterated, expected);
}

#[test]
fn fuzz_world_across_optimize() {
    let mut rng = Rng(0xD1B5_4A32_D192_ED03);
    let mut world = World::new(3);
    let mut reference: HashMap<(i32, i32, i32), BlockState> = HashMap::new();

    for op in 0..150_000 {
        let x = -48 + rng.below(112) as i32;
        let z = -48 + rng.below(112) as i32;
        let y = pick_y(&mut rng);
        let state = pick_state(&mut rng);

        world.set_block((x, y, z), state);
        reference.insert((x, y, z), state);

        if op % 5_000 == 4_999 {
            world.optimize();
            for (&pos, &expected) in &reference {
                assert_eq!(
                    world.get_block(pos),
                    Some(expected),
                    "pos ({}, {}, {})",
                    pos.0,
                    pos.1,
                    pos.2
                );
            }
        }
    }

    assert_eq!(world.get_block((64, 0, 0)), None);
    assert_eq!(world.get_block((-49, 0, 0)), None);
}
