use mcre_world::{Block, BlockEntity, Chunk, ChunkPos, WORLD_MIN_Y};

#[test]
fn test_set_get_roundtrip() {
    let mut chunk = Chunk::new();
    let stone = Block::STONE.default_state();
    let oak_log = Block::OAK_LOG.default_state();

    chunk.set_block(0, 0, 0, stone);
    chunk.set_block(15, WORLD_MIN_Y, 15, stone);
    chunk.set_block(3, 64, 7, oak_log);
    chunk.set_block(1, WORLD_MIN_Y + 383, 2, oak_log);

    assert_eq!(chunk.get_block(0, 0, 0), stone);
    assert_eq!(chunk.get_block(15, WORLD_MIN_Y, 15), stone);
    assert_eq!(chunk.get_block(3, 64, 7), oak_log);
    assert_eq!(chunk.get_block(1, WORLD_MIN_Y + 383, 2), oak_log);
    assert_eq!(chunk.get_block(4, 64, 7), Block::AIR.default_state());
}

#[test]
fn test_out_of_height_is_air() {
    let chunk = Chunk::new();
    assert!(chunk.get_block(0, WORLD_MIN_Y - 1, 0).is_air());
    assert!(chunk.get_block(0, WORLD_MIN_Y + 384, 0).is_air());
    assert!(chunk.get_block(0, 0, 0).is_air());

    let mut chunk = Chunk::new();
    chunk.set_block(0, WORLD_MIN_Y - 1, 0, Block::STONE.default_state());
    assert!(chunk.get_block(0, WORLD_MIN_Y - 1, 0).is_air());
}

#[test]
fn test_iter_blocks() {
    let mut chunk = Chunk::new();
    chunk.set_block(1, 2, 3, Block::STONE.default_state());
    chunk.set_block(4, WORLD_MIN_Y + 200, 5, Block::DIRT.default_state());

    let blocks: Vec<_> = chunk.iter_blocks().collect();
    assert_eq!(blocks.len(), 2);
    assert!(blocks.contains(&(1, 2, 3, Block::STONE.default_state())));
    assert!(blocks.contains(&(4, WORLD_MIN_Y + 200, 5, Block::DIRT.default_state())));
}

#[test]
fn test_chunk_pos() {
    let pos = ChunkPos::from_block(17, -1);
    assert_eq!(pos, ChunkPos::new(1, -1));
    assert_eq!(pos.min_block_x(), 16);
    assert_eq!(pos.min_block_z(), -16);
}

#[test]
fn chunk_block_entities_round_trip_and_clear_on_block_change() {
    let mut chunk = Chunk::new_at(ChunkPos::new(-2, 3));
    let position = (-31, 64, 49);
    let entity = BlockEntity::new("minecraft:chest".into(), position, vec![1, 2, 3]);

    assert!(chunk.set_block_entity(entity.clone()));
    assert_eq!(chunk.block_entity_at(position), Some(&entity));
    assert_eq!(chunk.clone().block_entities(), &[entity]);
    assert!(!chunk.set_block_entity(BlockEntity::new(
        "minecraft:chest".into(),
        (0, 64, 49),
        Vec::new(),
    )));

    chunk.set_block(1, 64, 1, Block::STONE.default_state());
    assert!(chunk.block_entities().is_empty());
}
