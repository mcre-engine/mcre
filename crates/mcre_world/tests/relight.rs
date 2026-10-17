use std::time::Instant;

use mcre_world::{
    Block, BlockState, Chunk, ChunkPos, PropVal, TypePropVal, WORLD_HEIGHT, WORLD_MIN_Y, World,
};

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

fn stone() -> BlockState {
    Block::STONE.default_state()
}

fn air() -> BlockState {
    Block::AIR.default_state()
}

fn glass() -> BlockState {
    Block::GLASS.default_state()
}

fn leaves() -> BlockState {
    Block::OAK_LEAVES.default_state()
}

fn glowstone() -> BlockState {
    Block::GLOWSTONE.default_state()
}

fn slab() -> BlockState {
    Block::STONE_SLAB.default_state()
}

fn slab_top() -> BlockState {
    Block::STONE_SLAB
        .state(&[PropVal::Type(TypePropVal::Top)])
        .unwrap()
}

fn random_state(rng: &mut Rng) -> BlockState {
    match rng.below(7) {
        0 => air(),
        1 => stone(),
        2 => glass(),
        3 => leaves(),
        4 => glowstone(),
        5 => slab(),
        _ => slab_top(),
    }
}

#[derive(Clone)]
enum Op {
    Set((i32, i32, i32), BlockState),
    AddChunk(Box<Chunk>),
}

fn apply(world: &mut World, op: &Op) {
    match op {
        Op::Set(pos, state) => world.set_block(*pos, *state),
        Op::AddChunk(chunk) => world.add_chunk((**chunk).clone()),
    }
}

fn assert_light_eq(a: &World, b: &World, op_count: usize) {
    for chunk in a.iter_chunks() {
        let origin_x = chunk.pos().x * 16;
        let origin_z = chunk.pos().z * 16;
        for lx in 0..16 {
            for lz in 0..16 {
                let x = origin_x + lx;
                let z = origin_z + lz;
                for y in WORLD_MIN_Y..WORLD_MIN_Y + WORLD_HEIGHT {
                    assert_eq!(
                        a.light_level((x, y, z)),
                        b.light_level((x, y, z)),
                        "light mismatch at ({x}, {y}, {z}) after {op_count} ops",
                    );
                }
            }
        }
    }
}

struct Model {
    radius: u32,
    ops: Vec<Op>,
}

impl Model {
    fn new(radius: u32) -> (Self, World) {
        (
            Self {
                radius,
                ops: Vec::new(),
            },
            World::new(radius),
        )
    }

    fn push(&mut self, world: &mut World, op: Op) {
        apply(world, &op);
        self.ops.push(op);
        world.compute_light();
        self.check(world);
    }

    fn extend(&mut self, world: &mut World, ops: Vec<Op>) {
        for op in ops {
            apply(world, &op);
            self.ops.push(op);
        }
        world.compute_light();
        self.check(world);
    }

    fn check(&self, world: &World) {
        let mut oracle = World::new(self.radius);
        for op in &self.ops {
            apply(&mut oracle, op);
        }
        oracle.compute_light_full();
        assert_light_eq(world, &oracle, self.ops.len());
    }
}

fn ring_pos(rng: &mut Rng, radius: u32) -> (i32, i32, i32) {
    let side = (2 * radius as i32 + 1) * 16;
    let min = -(radius as i32) * 16;
    let x = min + rng.below(side as u64) as i32;
    let z = min + rng.below(side as u64) as i32;
    let y = WORLD_MIN_Y + rng.below(WORLD_HEIGHT as u64) as i32;
    (x, y, z)
}

#[test]
fn relight_open_sky_matches_full() {
    let mut rng = Rng(0x1234_5678_9ABC_DEF0);
    let (mut model, mut world) = Model::new(0);
    for _ in 0..120 {
        let pos = ring_pos(&mut rng, 0);
        model.push(&mut world, Op::Set(pos, random_state(&mut rng)));
    }
}

#[test]
fn relight_ceiling_darkens_and_matches_full() {
    let (mut model, mut world) = Model::new(0);
    world.compute_light();
    assert_eq!(world.sky_light((8, 0, 8)), 15);
    let mut ceiling = Vec::new();
    for x in 6..=10 {
        for z in 6..=10 {
            ceiling.push(Op::Set((x, 20, z), stone()));
        }
    }
    model.extend(&mut world, ceiling);
    assert_eq!(world.sky_light((8, 21, 8)), 15);
    assert!(world.sky_light((8, 19, 8)) < 15);
    assert!(world.sky_light((8, -64, 8)) < 15);
    model.push(&mut world, Op::Set((8, 20, 8), air()));
    assert_eq!(world.sky_light((8, 19, 8)), 15);
    let mut rng = Rng(0x0BAD_F00D_CAFE_BABE);
    for _ in 0..60 {
        let pos = ring_pos(&mut rng, 0);
        model.push(&mut world, Op::Set(pos, random_state(&mut rng)));
    }
}

#[test]
fn relight_stone_box_matches_full() {
    let (mut model, mut world) = Model::new(0);
    let mut walls = Vec::new();
    for x in 3..=12 {
        for z in 3..=12 {
            for y in 0..=5 {
                let wall = x == 3 || x == 12 || z == 3 || z == 12 || y == 0 || y == 5;
                if wall {
                    walls.push(Op::Set((x, y, z), stone()));
                }
            }
        }
    }
    model.extend(&mut world, walls);
    assert_eq!(world.sky_light((7, 4, 7)), 0);
    model.push(&mut world, Op::Set((7, 1, 7), glowstone()));
    assert_eq!(world.block_light((7, 1, 7)), 15);
    assert_eq!(world.sky_light((7, 1, 7)), 0);
    model.push(&mut world, Op::Set((7, 5, 7), glass()));
    model.push(&mut world, Op::Set((7, 1, 7), air()));
    assert_eq!(world.block_light((7, 1, 7)), 0);
    let mut rng = Rng(0xFEED_FACE_1234_5678);
    for i in 0..60 {
        let pos = if i % 2 == 0 {
            (
                4 + rng.below(8) as i32,
                1 + rng.below(3) as i32,
                4 + rng.below(8) as i32,
            )
        } else {
            ring_pos(&mut rng, 0)
        };
        model.push(&mut world, Op::Set(pos, random_state(&mut rng)));
    }
}

#[test]
fn relight_glowstone_matches_full() {
    let (mut model, mut world) = Model::new(0);
    let pillar: Vec<Op> = (0..8).map(|y| Op::Set((8, y, 8), glowstone())).collect();
    model.extend(&mut world, pillar);
    assert_eq!(world.block_light((8, 0, 8)), 15);
    let mut rng = Rng(0xC0FF_EE11_2233_4455);
    for _ in 0..40 {
        let pos = ring_pos(&mut rng, 0);
        model.push(&mut world, Op::Set(pos, random_state(&mut rng)));
    }
    for y in 0..8 {
        model.push(&mut world, Op::Set((8, y, 8), air()));
    }
    assert_eq!(world.block_light((8, 0, 8)), 0);
}

#[test]
fn relight_multi_chunk_matches_full() {
    let (mut model, mut world) = Model::new(1);
    let mut setup = Vec::new();
    for z in -8..=8 {
        for y in 0..=3 {
            setup.push(Op::Set((15, y, z), stone()));
            setup.push(Op::Set((16, y, z), stone()));
        }
    }
    setup.push(Op::Set((-16, 1, -16), glowstone()));
    setup.push(Op::Set((31, 2, 31), glowstone()));
    setup.push(Op::Set((0, 10, 0), leaves()));
    model.extend(&mut world, setup);
    let mut rng = Rng(0xA11C_EB0B_5EED_1234);
    for _ in 0..10 {
        let x = rng.below(8) as i32 - 4;
        let z = rng.below(8) as i32 + 12;
        let y = WORLD_MIN_Y + rng.below(WORLD_HEIGHT as u64) as i32;
        model.push(&mut world, Op::Set((x, y, z), random_state(&mut rng)));
    }
    for _ in 0..10 {
        let pos = ring_pos(&mut rng, 1);
        model.push(&mut world, Op::Set(pos, random_state(&mut rng)));
    }
}

#[test]
fn relight_chunk_add_matches_full() {
    let (mut model, mut world) = Model::new(1);
    let mut rng = Rng(0xDADD_1E55_0102_0304);
    for _ in 0..8 {
        let pos = ring_pos(&mut rng, 1);
        model.push(&mut world, Op::Set(pos, random_state(&mut rng)));
    }
    let mut chunk = Chunk::new_at(ChunkPos::new(1, 0));
    for lx in 0..16 {
        for lz in 0..16 {
            chunk.set_block(lx, 4, lz, stone());
        }
    }
    chunk.set_block(8, 5, 8, glowstone());
    model.push(&mut world, Op::AddChunk(Box::new(chunk)));
    assert_eq!(world.sky_light((24, 6, 8)), 15);
    assert_eq!(world.block_light((24, 5, 8)), 15);
    for _ in 0..8 {
        let pos = ring_pos(&mut rng, 1);
        model.push(&mut world, Op::Set(pos, random_state(&mut rng)));
    }
    model.push(
        &mut world,
        Op::AddChunk(Box::new(Chunk::new_at(ChunkPos::new(2, 0)))),
    );
    model.push(&mut world, Op::Set((32, 0, 0), glowstone()));
    assert_eq!(world.block_light((32, 0, 0)), 15);
    for _ in 0..8 {
        let pos = ring_pos(&mut rng, 1);
        model.push(&mut world, Op::Set(pos, random_state(&mut rng)));
    }
}

#[test]
fn relight_single_edit_faster_than_full() {
    let mut ops = Vec::new();
    for x in -40..40 {
        for z in -40..40 {
            ops.push(Op::Set((x, 0, z), stone()));
        }
    }
    for x in -8..=8 {
        ops.push(Op::Set((x, 6, -8), stone()));
        ops.push(Op::Set((x, 6, 8), stone()));
    }
    ops.push(Op::Set((0, 1, 0), glowstone()));
    ops.push(Op::Set((10, 1, 10), glowstone()));
    ops.push(Op::Set((-10, 8, 5), glass()));
    let build = || {
        let mut world = World::new(2);
        for op in &ops {
            apply(&mut world, op);
        }
        world
    };
    let mut incremental = build();
    incremental.compute_light();
    let mut full_times = Vec::new();
    for _ in 0..2 {
        let mut world = build();
        let start = Instant::now();
        world.compute_light();
        full_times.push(start.elapsed());
    }
    let full = *full_times.iter().min().unwrap();
    incremental.set_block((4, 10, -3), stone());
    let mut incremental_times = Vec::new();
    for _ in 0..5 {
        let start = Instant::now();
        incremental.compute_light();
        incremental_times.push(start.elapsed());
        incremental.set_block((4, 10, -3), stone());
    }
    let single = *incremental_times.iter().min().unwrap();
    eprintln!("full recompute: {full:?}, incremental single edit: {single:?}");
    let mut oracle = build();
    oracle.set_block((4, 10, -3), stone());
    oracle.compute_light_full();
    assert_light_eq(&incremental, &oracle, 0);
    assert!(
        single < full,
        "incremental single edit ({single:?}) should beat whole-ring recompute ({full:?})"
    );
}
