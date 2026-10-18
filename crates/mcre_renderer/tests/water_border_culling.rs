use mcre_renderer::{Atlas, AtlasRegion, LightMode, Mesh, MeshOptions, mesh_chunk};
use mcre_resource_pack::{Colormaps, ResourcePackData};
use mcre_world::fluid::{FluidKind, fluid_state};
use mcre_world::{Block, BlockState, Chunk, ChunkPos, PropVal, World};

fn data() -> ResourcePackData {
    ResourcePackData {
        fluid_textures: Default::default(),
        entries: vec![],
        texture_table: vec![],
        culling: Default::default(),
        colormaps: Colormaps {
            grass: vec![0u8; 65536 * 3].into_boxed_slice(),
            foliage: vec![0u8; 65536 * 3].into_boxed_slice(),
            dry_foliage: vec![0u8; 65536 * 3].into_boxed_slice(),
        },
    }
}

fn atlas() -> Atlas {
    Atlas {
        width: 16,
        height: 16,
        pixels: vec![],
        regions: vec![],
        fallback: AtlasRegion {
            x: 0,
            y: 0,
            width: 16,
            height: 16,
        },
    }
}

fn water(level: u8) -> BlockState {
    Block::WATER
        .state(&[PropVal::Level(level)])
        .expect("water level")
}

#[test]
fn water_faces_are_culled_across_chunk_borders() {
    let mut world = World::new(1);
    for (cx, cz) in [
        (-1, -1),
        (0, -1),
        (1, -1),
        (-1, 0),
        (0, 0),
        (1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
    ] {
        let mut chunk = Chunk::new_at(ChunkPos::new(cx, cz));
        for x in 0..16 {
            for z in 0..16 {
                for y in 50..63 {
                    chunk.set_block(x, y, z, Block::STONE.default_state());
                }
                chunk.set_block(x, 63, z, Block::WATER.default_state());
            }
        }
        world.replace_chunk(chunk);
    }
    assert_eq!(
        world.get_block((16, 63, 8)),
        Some(Block::WATER.default_state())
    );

    let data = data();
    let atlas = atlas();
    let chunk = world.chunk_at_pos(ChunkPos::new(0, 0)).unwrap();
    let mesh = mesh_chunk(
        &data,
        &atlas,
        chunk,
        &world,
        [0, 0],
        LightMode::Flat,
        MeshOptions::default(),
    );

    let mut vertical_at_border = 0;
    for quad in mesh.translucent_indices.chunks(6) {
        let p: Vec<[f32; 3]> = quad
            .iter()
            .map(|&i| mesh.vertices[i as usize].position)
            .collect();
        let normal = cross(sub(p[1], p[0]), sub(p[2], p[1]));
        let vertical = normal[1].abs() < 1e-3;
        let at_x16 = p.iter().all(|v| (v[0] - 16.0).abs() < 1e-3);
        let at_z16 = p.iter().all(|v| (v[2] - 16.0).abs() < 1e-3);
        if vertical && (at_x16 || at_z16) {
            vertical_at_border += 1;
        }
    }
    eprintln!(
        "translucent quads: {}, vertical border quads: {}",
        mesh.translucent_indices.len() / 6,
        vertical_at_border
    );
    assert_eq!(
        vertical_at_border, 0,
        "water faces emitted at chunk borders"
    );
}

#[test]
fn ocean_with_island_has_no_internal_water_faces() {
    let mut world = World::new(1);
    for cx in -1..=1 {
        for cz in -1..=1 {
            let mut chunk = Chunk::new_at(ChunkPos::new(cx, cz));
            build_ocean(&mut chunk, cx * 16, cz * 16);
            world.replace_chunk(chunk);
        }
    }

    let data = data();
    let atlas = atlas();
    let mut total = 0;
    for cx in -1..=1 {
        for cz in -1..=1 {
            let chunk = world.chunk_at_pos(ChunkPos::new(cx, cz)).unwrap();
            let mesh = mesh_chunk(
                &data,
                &atlas,
                chunk,
                &world,
                [cx * 16, cz * 16],
                LightMode::Flat,
                MeshOptions::default(),
            );
            total += internal_water_faces(&mesh, &world);
        }
    }
    assert_eq!(total, 0, "internal water faces emitted inside the ocean");
}

#[test]
fn water_cube_has_no_internal_water_faces() {
    let mut world = World::new(1);
    let mut chunk = Chunk::new();
    for x in 2..=4 {
        for z in 3..=5 {
            chunk.set_block(x, 11, z, Block::STONE.default_state());
            for y in 12..=14 {
                chunk.set_block(x, y, z, water(0));
            }
        }
    }
    world.replace_chunk(chunk);

    let data = data();
    let atlas = atlas();
    let mesh = mesh_chunk(
        &data,
        &atlas,
        world.chunk_at_pos(ChunkPos::new(0, 0)).unwrap(),
        &world,
        [0, 0],
        LightMode::Flat,
        MeshOptions::default(),
    );

    assert_eq!(internal_water_faces(&mesh, &world), 0);
}

const OCEAN_SURFACE_Y: i32 = 6;

fn ocean_floor(x: i32, z: i32) -> i32 {
    let trench = ((-2..=1).contains(&x) && (2..=12).contains(&z))
        || ((4..=12).contains(&x) && (-2..=1).contains(&z));
    if trench { -1 } else { 2 }
}

fn island_height(x: i32, z: i32) -> Option<i32> {
    let r = (x - 16).abs().max((z + 8).abs());
    match r {
        0 => Some(9),
        1 => Some(8),
        2 => Some(7),
        3 => Some(6),
        4 => Some(4),
        _ => None,
    }
}

fn build_ocean(chunk: &mut Chunk, ox: i32, oz: i32) {
    for x in ox..ox + 16 {
        for z in oz..oz + 16 {
            let floor = ocean_floor(x, z);
            for y in -3..floor {
                chunk.set_block(x - ox, y, z - oz, Block::STONE.default_state());
            }
            chunk.set_block(x - ox, floor, z - oz, Block::SAND.default_state());
            for y in floor + 1..=OCEAN_SURFACE_Y {
                chunk.set_block(x - ox, y, z - oz, water(0));
            }
            if let Some(height) = island_height(x, z) {
                for y in floor + 1..height {
                    chunk.set_block(x - ox, y, z - oz, Block::SAND.default_state());
                }
                let r = (x - 16).abs().max((z + 8).abs());
                let top = if r <= 1 {
                    Block::GRASS_BLOCK
                } else {
                    Block::SAND
                };
                chunk.set_block(x - ox, height, z - oz, top.default_state());
            }
        }
    }
}

fn is_water(block: BlockState) -> bool {
    fluid_state(block).is_some_and(|fluid| fluid.kind == FluidKind::Water)
}

fn internal_water_faces(mesh: &Mesh, world: &World) -> usize {
    let mut internal = 0;
    for quad in mesh.translucent_indices.chunks(6) {
        let p: Vec<[f32; 3]> = quad
            .iter()
            .map(|&i| mesh.vertices[i as usize].position)
            .collect();
        let mut n = cross(sub(p[1], p[0]), sub(p[2], p[1]));
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if len < 1e-8 {
            continue;
        }
        n = [n[0] / len, n[1] / len, n[2] / len];
        let c = [
            (p[0][0] + p[1][0] + p[2][0] + p[3][0]) / 4.0,
            (p[0][1] + p[1][1] + p[2][1] + p[3][1]) / 4.0,
            (p[0][2] + p[1][2] + p[2][2] + p[3][2]) / 4.0,
        ];
        let front = world.get_block((
            (c[0] + n[0] * 0.55).floor() as i32,
            (c[1] + n[1] * 0.55).floor() as i32,
            (c[2] + n[2] * 0.55).floor() as i32,
        ));
        let back = world.get_block((
            (c[0] - n[0] * 0.55).floor() as i32,
            (c[1] - n[1] * 0.55).floor() as i32,
            (c[2] - n[2] * 0.55).floor() as i32,
        ));
        if front.is_some_and(is_water) && back.is_some_and(is_water) {
            internal += 1;
        }
    }
    internal
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
