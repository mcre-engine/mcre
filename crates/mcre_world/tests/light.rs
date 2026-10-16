use mcre_world::{Block, BlockState, ChunkLight, World};

fn set_full_layer(world: &mut World, y: i32, state: BlockState) {
    for x in 0..16 {
        for z in 0..16 {
            world.set_block((x, y, z), state);
        }
    }
}

#[test]
fn open_world_is_lit_by_sky() {
    let mut world = World::new(0);
    world.compute_light();

    assert_eq!(world.sky_light((0, 0, 0)), 15);
    assert_eq!(world.sky_light((15, 200, 15)), 15);
    assert_eq!(world.block_light((8, 8, 8)), 0);
}

#[test]
fn stone_layer_blocks_skylight() {
    let mut world = World::new(0);
    set_full_layer(&mut world, 10, Block::STONE.default_state());
    world.compute_light();

    assert_eq!(world.sky_light((5, 11, 5)), 15);
    assert_eq!(world.sky_light((5, 10, 5)), 0);
    assert_eq!(world.sky_light((5, 9, 5)), 0);
    assert_eq!(world.sky_light((5, -64, 5)), 0);
}

#[test]
fn glowstone_block_light_decays_with_distance() {
    let mut world = World::new(0);
    world.set_block((8, 0, 8), Block::GLOWSTONE.default_state());
    world.compute_light();

    assert_eq!(world.block_light((8, 0, 8)), 15);
    assert_eq!(world.block_light((9, 0, 8)), 14);
    assert_eq!(world.block_light((13, 0, 8)), 10);
    assert_eq!(world.block_light((8, 3, 8)), 12);
}

#[test]
fn unloaded_positions_default_to_open_sky() {
    let world = World::new(0);

    assert_eq!(world.sky_light((100, 0, 100)), 15);
    assert_eq!(world.block_light((100, 0, 100)), 0);
}

#[test]
fn nibble_roundtrip() {
    let mut light = ChunkLight::new();

    assert_eq!(light.sky_level(3, 5, 7), 0);
    light.set_sky_level(3, 5, 7, 9);
    assert_eq!(light.sky_level(3, 5, 7), 9);

    light.set_block_level(15, 300, 0, 3);
    assert_eq!(light.block_level(15, 300, 0), 3);
    assert_eq!(light.block_level(14, 300, 0), 0);

    light.set_sky_level(3, 5, 7, 0);
    assert_eq!(light.sky_level(3, 5, 7), 0);

    light.set_sky_level(0, 100, 0, 0);
    assert_eq!(light.sky_level(0, 100, 0), 0);

    assert_eq!(light.sky_level(0, -65, 0), 0);
}

#[test]
fn combined_light_level_matches_channels() {
    let mut world = World::new(0);
    world.set_block((8, 0, 8), Block::GLOWSTONE.default_state());
    world.compute_light();

    assert_eq!(world.light_level((8, 0, 8)), (15, 0));
    assert_eq!(world.light_level((9, 0, 8)), (14, 15));
    assert_eq!(world.light_level((100, 0, 100)), (0, 15));
}
