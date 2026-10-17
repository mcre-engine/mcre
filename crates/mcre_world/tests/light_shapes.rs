use mcre_core::Direction;
use mcre_world::shape::face_boxes;
use mcre_world::{
    Block, BlockState, HalfPropVal, PropVal, TypePropVal, World, is_empty_shape, shape_occludes,
};

const DIRS: [Direction; 6] = [
    Direction::Down,
    Direction::Up,
    Direction::North,
    Direction::South,
    Direction::West,
    Direction::East,
];

const NAMES: [&str; 16] = [
    "air",
    "stone",
    "glass",
    "ice",
    "water",
    "oak_leaves",
    "stone_slab_bottom",
    "stone_slab_top",
    "oak_stairs_bottom",
    "oak_stairs_top",
    "oak_fence",
    "glass_pane",
    "iron_bars",
    "torch",
    "snow_layer",
    "white_carpet",
];

/// Vanilla 26.4-snapshot-4 `LightEngine.shapeOccludes` probe results: per
/// from-state per direction (down/up/north/south/west/east) a bitmask of
/// to-state indices whose face pair occludes.
const PROBE_OCCLUDES: [[u16; 6]; 16] = [
    [640, 16704, 0, 768, 0, 0],
    [640, 16704, 0, 768, 0, 0],
    [640, 16704, 0, 768, 0, 0],
    [640, 16704, 0, 768, 0, 0],
    [640, 16704, 0, 768, 0, 0],
    [640, 16704, 0, 768, 0, 0],
    [65535, 16704, 640, 896, 640, 640],
    [640, 65535, 320, 832, 320, 320],
    [65535, 16704, 65535, 896, 640, 640],
    [640, 65535, 65535, 832, 320, 320],
    [640, 16704, 0, 768, 0, 0],
    [640, 16704, 0, 768, 0, 0],
    [640, 16704, 0, 768, 0, 0],
    [640, 16704, 0, 768, 0, 0],
    [65535, 16704, 0, 768, 0, 0],
    [640, 16704, 0, 768, 0, 0],
];

/// Vanilla probe `isEmptyShape` flags (states with `useShapeForLightOcclusion`
/// false or `canOcclude` false read as empty occlusion shapes).
const PROBE_EMPTY: [bool; 16] = [
    true, true, true, true, true, true, false, false, false, false, true, true, true, true, false,
    true,
];

fn probe_states() -> [BlockState; 16] {
    [
        Block::AIR.default_state(),
        Block::STONE.default_state(),
        Block::GLASS.default_state(),
        Block::ICE.default_state(),
        Block::WATER.default_state(),
        Block::OAK_LEAVES.default_state(),
        Block::STONE_SLAB.default_state(),
        Block::STONE_SLAB
            .state(&[PropVal::Type(TypePropVal::Top)])
            .unwrap(),
        Block::OAK_STAIRS.default_state(),
        Block::OAK_STAIRS
            .state(&[PropVal::Half(HalfPropVal::Top)])
            .unwrap(),
        Block::OAK_FENCE.default_state(),
        Block::GLASS_PANE.default_state(),
        Block::IRON_BARS.default_state(),
        Block::TORCH.default_state(),
        Block::SNOW.default_state(),
        Block::WHITE_CARPET.default_state(),
    ]
}

#[test]
fn probe_states_match_vanilla_properties() {
    let states = probe_states();
    for (i, state) in states.iter().enumerate() {
        assert_eq!(is_empty_shape(*state), PROBE_EMPTY[i], "{}", NAMES[i]);
    }
    assert_eq!(states[6].light_dampening(), 0);
    assert!(states[6].use_shape_for_light_occlusion());
    assert!(states[6].can_occlude());
    assert_eq!(states[7].slab_type(), mcre_world::SlabType::Top);
    assert_eq!(states[9].half(), mcre_world::Half::Top);
    assert_eq!(states[14].layers(), 1);
}

#[test]
fn shape_occludes_matches_vanilla_probe() {
    let states = probe_states();
    for (i, from) in states.iter().enumerate() {
        for (d, direction) in DIRS.iter().enumerate() {
            let mut mask = 0u16;
            for (j, to) in states.iter().enumerate() {
                if shape_occludes(*from, *to, *direction) {
                    mask |= 1 << j;
                }
            }
            assert_eq!(
                mask, PROBE_OCCLUDES[i][d],
                "from {} direction {direction:?}",
                NAMES[i]
            );
        }
    }
}

enum ExpectedFace {
    Empty,
    Full,
    Boxes(&'static [[f32; 6]]),
}

fn light_face(state: BlockState, direction: Direction) -> Vec<[f32; 6]> {
    if is_empty_shape(state) {
        Vec::new()
    } else {
        face_boxes(state.occlusion_boxes(), direction)
    }
}

fn boxes_eq(a: &[[f32; 6]], b: &[[f32; 6]]) -> bool {
    mcre_world::shape::is_covered(a, b) && mcre_world::shape::is_covered(b, a)
}

#[test]
fn face_occlusion_shapes_match_vanilla_probe() {
    const FULL: [[f32; 6]; 1] = [[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]];
    const SLAB_BOTTOM_SIDES: [[f32; 6]; 1] = [[0.0, 0.0, 0.0, 1.0, 0.5, 1.0]];
    const SLAB_TOP_SIDES: [[f32; 6]; 1] = [[0.0, 0.5, 0.0, 1.0, 1.0, 1.0]];
    const STAIRS_BOTTOM_UP: [[f32; 6]; 1] = [[0.0, 0.0, 0.0, 1.0, 1.0, 0.5]];
    const STAIRS_BOTTOM_SOUTH: [[f32; 6]; 1] = [[0.0, 0.0, 0.0, 1.0, 0.5, 1.0]];
    const STAIRS_BOTTOM_SIDES: [[f32; 6]; 2] = [
        [0.0, 0.0, 0.0, 1.0, 0.5, 1.0],
        [0.0, 0.5, 0.0, 1.0, 1.0, 0.5],
    ];
    const STAIRS_TOP_DOWN: [[f32; 6]; 1] = [[0.0, 0.0, 0.0, 1.0, 1.0, 0.5]];
    const STAIRS_TOP_SOUTH: [[f32; 6]; 1] = [[0.0, 0.5, 0.0, 1.0, 1.0, 1.0]];
    const STAIRS_TOP_SIDES: [[f32; 6]; 2] = [
        [0.0, 0.0, 0.0, 1.0, 1.0, 0.5],
        [0.0, 0.5, 0.5, 1.0, 1.0, 1.0],
    ];
    const SNOW_SIDES: [[f32; 6]; 1] = [[0.0, 0.0, 0.0, 1.0, 0.125, 1.0]];

    let states = probe_states();
    let cases: &[(usize, Direction, ExpectedFace)] = &[
        (1, Direction::Down, ExpectedFace::Empty),
        (1, Direction::Up, ExpectedFace::Empty),
        (6, Direction::Down, ExpectedFace::Full),
        (6, Direction::Up, ExpectedFace::Empty),
        (6, Direction::North, ExpectedFace::Boxes(&SLAB_BOTTOM_SIDES)),
        (6, Direction::South, ExpectedFace::Boxes(&SLAB_BOTTOM_SIDES)),
        (6, Direction::West, ExpectedFace::Boxes(&SLAB_BOTTOM_SIDES)),
        (6, Direction::East, ExpectedFace::Boxes(&SLAB_BOTTOM_SIDES)),
        (7, Direction::Down, ExpectedFace::Empty),
        (7, Direction::Up, ExpectedFace::Full),
        (7, Direction::North, ExpectedFace::Boxes(&SLAB_TOP_SIDES)),
        (8, Direction::Down, ExpectedFace::Full),
        (8, Direction::Up, ExpectedFace::Boxes(&STAIRS_BOTTOM_UP)),
        (8, Direction::North, ExpectedFace::Full),
        (
            8,
            Direction::South,
            ExpectedFace::Boxes(&STAIRS_BOTTOM_SOUTH),
        ),
        (
            8,
            Direction::West,
            ExpectedFace::Boxes(&STAIRS_BOTTOM_SIDES),
        ),
        (
            8,
            Direction::East,
            ExpectedFace::Boxes(&STAIRS_BOTTOM_SIDES),
        ),
        (9, Direction::Down, ExpectedFace::Boxes(&STAIRS_TOP_DOWN)),
        (9, Direction::Up, ExpectedFace::Full),
        (9, Direction::North, ExpectedFace::Full),
        (9, Direction::South, ExpectedFace::Boxes(&STAIRS_TOP_SOUTH)),
        (9, Direction::West, ExpectedFace::Boxes(&STAIRS_TOP_SIDES)),
        (9, Direction::East, ExpectedFace::Boxes(&STAIRS_TOP_SIDES)),
        (10, Direction::North, ExpectedFace::Empty),
        (14, Direction::Down, ExpectedFace::Full),
        (14, Direction::Up, ExpectedFace::Empty),
        (14, Direction::North, ExpectedFace::Boxes(&SNOW_SIDES)),
    ];

    for (i, direction, expected) in cases {
        let boxes = light_face(states[*i], *direction);
        match expected {
            ExpectedFace::Empty => assert!(boxes.is_empty(), "{} {direction:?}", NAMES[*i]),
            ExpectedFace::Full => {
                assert!(boxes_eq(&boxes, &FULL), "{} {direction:?}", NAMES[*i])
            }
            ExpectedFace::Boxes(expected) => {
                assert!(boxes_eq(&boxes, expected), "{} {direction:?}", NAMES[*i])
            }
        }
    }
}

fn sealed_world(shaft_down: bool, shaft_up: bool) -> World {
    let mut world = World::new(0);
    for x in 6..=11 {
        for z in 7..=9 {
            for y in 99..=101 {
                world.set_block((x, y, z), Block::STONE.default_state());
            }
        }
    }
    for x in 7..=10 {
        world.set_block((x, 100, 8), Block::AIR.default_state());
    }
    world.set_block((7, 100, 8), Block::GLOWSTONE.default_state());
    if shaft_down {
        world.set_block((8, 99, 8), Block::AIR.default_state());
    }
    if shaft_up {
        world.set_block((8, 101, 8), Block::AIR.default_state());
    }
    world
}

#[test]
fn bottom_slab_blocks_block_light_downwards() {
    let mut world = sealed_world(true, false);
    world.set_block((8, 100, 8), Block::STONE_SLAB.default_state());
    world.compute_light_full();
    assert_eq!(world.light_level((8, 100, 8)).0, 14);
    assert_eq!(world.light_level((8, 99, 8)).0, 0);

    let mut world = sealed_world(true, false);
    world.set_block(
        (8, 100, 8),
        Block::STONE_SLAB
            .state(&[PropVal::Type(TypePropVal::Top)])
            .unwrap(),
    );
    world.compute_light_full();
    assert_eq!(world.light_level((8, 100, 8)).0, 14);
    assert_eq!(world.light_level((8, 99, 8)).0, 13);
}

#[test]
fn top_slab_blocks_block_light_upwards() {
    let mut world = sealed_world(false, true);
    world.set_block((8, 100, 8), Block::STONE_SLAB.default_state());
    world.compute_light_full();
    assert_eq!(world.light_level((8, 100, 8)).0, 14);
    assert_eq!(world.light_level((8, 101, 8)).0, 13);

    let mut world = sealed_world(false, true);
    world.set_block(
        (8, 100, 8),
        Block::STONE_SLAB
            .state(&[PropVal::Type(TypePropVal::Top)])
            .unwrap(),
    );
    world.compute_light_full();
    assert_eq!(world.light_level((8, 100, 8)).0, 14);
    assert_eq!(world.light_level((8, 101, 8)).0, 0);
}

#[test]
fn bottom_slab_keeps_sky_column_but_not_below_itself() {
    let mut world = World::new(0);
    world.set_block((8, 100, 8), Block::STONE_SLAB.default_state());
    world.compute_light_full();
    assert_eq!(world.light_level((8, 100, 8)).1, 15);
    assert_eq!(world.light_level((8, 101, 8)).1, 15);
    assert_eq!(world.light_level((8, 99, 8)).1, 14);

    let mut world = World::new(0);
    world.set_block(
        (8, 100, 8),
        Block::STONE_SLAB
            .state(&[PropVal::Type(TypePropVal::Top)])
            .unwrap(),
    );
    world.compute_light_full();
    assert_eq!(world.light_level((8, 101, 8)).1, 15);
    assert_eq!(world.light_level((8, 100, 8)).1, 14);

    let mut world = World::new(0);
    world.set_block((8, 100, 8), Block::OAK_STAIRS.default_state());
    world.compute_light_full();
    assert_eq!(world.light_level((8, 100, 8)).1, 15);
    assert_eq!(world.light_level((8, 99, 8)).1, 14);
}

#[test]
fn removing_shape_block_restores_column() {
    let mut world = World::new(0);
    world.set_block((8, 100, 8), Block::STONE_SLAB.default_state());
    world.compute_light_full();
    assert_eq!(world.light_level((8, 99, 8)).1, 14);

    world.set_block((8, 100, 8), Block::AIR.default_state());
    world.compute_light();
    assert_eq!(world.light_level((8, 100, 8)).1, 15);
    assert_eq!(world.light_level((8, 99, 8)).1, 15);

    world.set_block((8, 101, 8), Block::STONE.default_state());
    world.compute_light();
    assert_eq!(world.light_level((8, 101, 8)).1, 0);
    assert_eq!(world.light_level((8, 100, 8)).1, 14);
    assert_eq!(world.light_level((8, 99, 8)).1, 14);
}
