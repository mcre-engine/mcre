use mcre_world::{Biome, Chunk, World};

#[test]
fn unset_cells_default_to_plains() {
    let chunk = Chunk::new();
    assert_eq!(chunk.biome_at(0, 0, 0), Biome::PLAINS);
    assert_eq!(chunk.biome_at(15, 300, 15), Biome::PLAINS);
    assert_eq!(chunk.biome_at(0, -1000, 0), Biome::PLAINS);
}

#[test]
fn biome_cells_roundtrip_per_4x4x4_cell() {
    let mut chunk = Chunk::new();
    chunk.set_biome(0, 0, 0, Biome::DESERT);
    chunk.set_biome(4, 0, 0, Biome::FOREST);
    chunk.set_biome(0, 4, 0, Biome::SAVANNA);

    assert_eq!(chunk.biome_at(0, 0, 0), Biome::DESERT);
    assert_eq!(chunk.biome_at(3, 3, 3), Biome::DESERT);
    assert_eq!(chunk.biome_at(4, 0, 0), Biome::FOREST);
    assert_eq!(chunk.biome_at(7, 3, 3), Biome::FOREST);
    assert_eq!(chunk.biome_at(0, 4, 0), Biome::SAVANNA);
    assert_eq!(chunk.biome_at(8, 0, 0), Biome::PLAINS);
}

#[test]
fn setting_plains_keeps_default_storage() {
    let mut chunk = Chunk::new();
    chunk.set_biome(0, 0, 0, Biome::PLAINS);
    assert_eq!(chunk.biome_at(0, 0, 0), Biome::PLAINS);
}

#[test]
fn world_biome_at_resolves_chunks() {
    let mut world = World::new(1);
    world.set_biome((0, 0, 0), Biome::DESERT);
    assert_eq!(world.biome_at((0, 0, 0)), Biome::DESERT);
    assert_eq!(world.biome_at((3, 0, 3)), Biome::DESERT);
    assert_eq!(world.biome_at((4, 0, 0)), Biome::PLAINS);
    assert_eq!(world.biome_at((100, 0, 100)), Biome::PLAINS);
}

#[test]
fn fill_biome_covers_all_sections() {
    let mut chunk = Chunk::new();
    chunk.fill_biome(Biome::DESERT);
    assert_eq!(chunk.biome_at(0, 0, 0), Biome::DESERT);
    assert_eq!(chunk.biome_at(15, 319, 15), Biome::DESERT);
    assert_eq!(chunk.biome_at(0, -64, 0), Biome::DESERT);

    chunk.fill_biome(Biome::PLAINS);
    assert_eq!(chunk.biome_at(0, 0, 0), Biome::PLAINS);
    assert_eq!(chunk.biome_at(15, 319, 15), Biome::PLAINS);
    assert_eq!(chunk.biome_at(0, -64, 0), Biome::PLAINS);
}
