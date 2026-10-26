use std::path::PathBuf;

use mcre_anvil::{BiomeResolver, BlockResolver, ImportStats, SavePaths, import_area, read_spawn};
use mcre_world::World;

fn main() {
    let save_dir = PathBuf::from(std::env::args().nth(1).expect("usage: inspect <save-dir>"));
    let spawn = read_spawn(&save_dir).expect("read spawn");
    println!(
        "spawn={:?} dim={} yaw={} pitch={}",
        spawn.pos, spawn.dimension, spawn.yaw, spawn.pitch
    );

    let paths = SavePaths::new(&save_dir);
    let region_dir = paths.resolve_region_dir(&spawn.dimension);
    println!("region_dir={}", region_dir.display());

    let spawn_chunk = (spawn.pos.0.div_euclid(16), spawn.pos.2.div_euclid(16));
    println!("spawn_chunk={spawn_chunk:?}");
    let radius = 1i32;

    let mut world = World::new(radius as u32);
    let blocks = BlockResolver::new();
    let biomes = BiomeResolver::new();
    let mut stats = ImportStats::default();
    import_area(
        &mut world,
        &region_dir,
        mcre_anvil::ImportRect {
            min_chunk: (spawn_chunk.0 - radius, spawn_chunk.1 - radius),
            max_chunk: (spawn_chunk.0 + radius, spawn_chunk.1 + radius),
            origin_chunk: spawn_chunk,
        },
        &blocks,
        &biomes,
        &mut stats,
    )
    .expect("import");
    println!(
        "chunks={} sections={} blocks_set={} skipped_sections={} skipped_chunks={}",
        stats.chunks,
        stats.sections,
        stats.blocks_set,
        stats.skipped_sections,
        stats.skipped_chunks,
    );
    println!("unknown_blocks={:?}", stats.unknown_blocks);
    println!("unknown_biomes={:?}", stats.unknown_biomes);

    let local = (
        spawn.pos.0 - spawn_chunk.0 * 16,
        spawn.pos.1,
        spawn.pos.2 - spawn_chunk.1 * 16,
    );
    println!("local spawn={local:?}");
    if let Some(state) = world.get_block(local) {
        println!(
            "block at spawn: {} air={}",
            state.block().name(),
            state.is_air()
        );
    } else {
        println!("block at spawn: OUT OF RING");
    }
    let mut top = None;
    for y in (mcre_world::WORLD_MIN_Y..mcre_world::WORLD_MIN_Y + mcre_world::WORLD_HEIGHT).rev() {
        let state = world.get_block((local.0, y, local.2)).unwrap();
        if !state.is_air() {
            top = Some((y, state.block().name().to_string()));
            break;
        }
    }
    println!("top solid at spawn column: {top:?}");
    let bedrock = world.get_block((local.0, -64, local.2)).unwrap();
    println!("block at y=-64: {}", bedrock.block().name());
    println!("biome at spawn: {}", world.biome_at(local).name());

    world.compute_light();
    let sky = world.sky_light((
        local.0,
        top.as_ref().map(|t| t.0 + 1).unwrap_or(local.1),
        local.2,
    ));
    println!("sky light above surface: {sky}");

    let edit = (
        local.0,
        top.as_ref().map(|t| t.0).unwrap_or(local.1),
        local.2,
    );
    let before = world.get_block(edit).unwrap();
    let t0 = std::time::Instant::now();
    let _cloned = world.clone();
    let clone_ms = t0.elapsed().as_secs_f64() * 1000.0;
    world.set_block(edit, mcre_world::Block::AIR.default_state());
    let dirty = world.take_dirty();
    let t0 = std::time::Instant::now();
    world.compute_light();
    let light_ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!(
        "edit benchmark (break surface): clone={clone_ms:.1}ms light={light_ms:.1}ms dirty={} (was {})",
        dirty.len(),
        before.block().name(),
    );
    world.acknowledge_light_edits(&[edit]);

    let cap = (
        local.0,
        top.as_ref().map(|t| t.0 + 5).unwrap_or(local.1),
        local.2,
    );
    world.set_block(cap, mcre_world::Block::STONE.default_state());
    let t0 = std::time::Instant::now();
    world.compute_light();
    println!(
        "edit benchmark (place stone in sky): light={:.1}ms",
        t0.elapsed().as_secs_f64() * 1000.0,
    );
    world.acknowledge_light_edits(&[cap]);

    let lamp = (
        local.0,
        top.as_ref().map(|t| t.0 + 1).unwrap_or(local.1),
        local.2,
    );
    world.set_block(lamp, mcre_world::Block::GLOWSTONE.default_state());
    let t0 = std::time::Instant::now();
    world.compute_light();
    println!(
        "edit benchmark (place glowstone): light={:.1}ms",
        t0.elapsed().as_secs_f64() * 1000.0,
    );
}
