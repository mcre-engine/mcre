use mcre_world::{Block, BlockState, Chunk, ChunkPos, World, WorldStorage};

fn stone() -> BlockState {
    Block::STONE.default_state()
}

fn dirt() -> BlockState {
    Block::DIRT.default_state()
}

fn air() -> BlockState {
    Block::AIR.default_state()
}

#[test]
fn test_manager_indexing() {
    let mut world = World::new(1);

    let center = (0, 0, 0);
    let corner = (16, 0, 16);

    world.set_block(center, stone());
    world.set_block(corner, dirt());

    assert_eq!(world.get_block(center), Some(stone()));
    assert_eq!(world.get_block(corner), Some(dirt()));
}

#[test]
fn test_manager_negative_coords() {
    let mut world = World::new(2);

    let neg_pos = (-1, 0, -1);
    world.set_block(neg_pos, stone());

    assert_eq!(world.get_block(neg_pos), Some(stone()));
}

#[test]
fn test_out_of_radius_is_rejected() {
    let mut world = World::new(1);

    // chunk (2, 0) wraps onto the slot of chunk (-1, 0): stale guard rejects it
    assert_eq!(world.get_block((32, 0, 0)), None);
    world.set_block((32, 0, 0), stone());
    assert_eq!(world.get_block((32, 0, 0)), None);

    // the in-radius chunk (-1, 0) still owns the slot
    assert_eq!(world.get_block((-16, 0, 0)), Some(air()));

    // installing chunk (2, 0) evicts chunk (-1, 0) from the slot
    world.add_chunk(Chunk::new_at(ChunkPos::new(2, 0)));
    world.set_block((32, 0, 0), stone());
    assert_eq!(world.get_block((32, 0, 0)), Some(stone()));
    assert_eq!(world.get_block((-16, 0, 0)), None);
}

#[test]
fn test_world_storage_trait() {
    fn roundtrip<W: WorldStorage>(world: &mut W) {
        world.set((5, 10, -7), stone());
        assert_eq!(world.get((5, 10, -7)), Some(stone()));
        assert_eq!(world.get((5, 10, -8)), Some(air()));
    }

    let mut world = World::new(1);
    roundtrip(&mut world);
}

#[test]
fn test_iter_chunks_covers_ring() {
    let world = World::new(2);

    let mut positions: Vec<_> = world.iter_chunks().map(|chunk| chunk.pos()).collect();
    positions.sort_by_key(|pos| (pos.x, pos.z));

    assert_eq!(positions.len(), 25);
    assert!(positions.contains(&ChunkPos::new(-2, -2)));
    assert!(positions.contains(&ChunkPos::new(0, 0)));
    assert!(positions.contains(&ChunkPos::new(2, 2)));
}

#[test]
fn set_block_marks_center_chunk_dirty() {
    let mut world = World::new(1);
    world.set_block((5, 0, 5), stone());

    assert_eq!(world.take_dirty(), vec![ChunkPos::new(0, 0)]);
    assert!(world.take_dirty().is_empty());
}

#[test]
fn border_edits_mark_neighbor_chunks() {
    let mut world = World::new(1);

    world.set_block((0, 0, 5), stone());
    let mut dirty = world.take_dirty();
    dirty.sort_by_key(|pos| (pos.x, pos.z));
    assert_eq!(dirty, vec![ChunkPos::new(-1, 0), ChunkPos::new(0, 0)]);

    world.set_block((15, 0, 15), stone());
    let mut dirty = world.take_dirty();
    dirty.sort_by_key(|pos| (pos.x, pos.z));
    assert_eq!(
        dirty,
        vec![
            ChunkPos::new(0, 0),
            ChunkPos::new(0, 1),
            ChunkPos::new(1, 0),
            ChunkPos::new(1, 1),
        ]
    );
}

#[test]
fn dirty_deduplicates_and_respects_radius() {
    let mut world = World::new(1);

    world.set_block((5, 0, 5), stone());
    world.set_block((5, 1, 5), stone());
    assert_eq!(world.take_dirty().len(), 1);

    world.set_block((32, 0, 0), stone());
    assert!(world.take_dirty().is_empty());
}
