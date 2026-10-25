use std::sync::OnceLock;

use mcre_fs::native::NativeFs;
use mcre_fs::{Fs, FsPath};
use mcre_renderer::{Atlas, AtlasRegion, LightMode, Mesh, MeshOptions, mesh_chunk};
use mcre_resource_pack::{ResourcePackData, ResourcePackSet};
use mcre_world::{Block, BlockState, Chunk, ChunkPos, World};

const COLUMN_X: f32 = 5.0;
const COLUMN_Z: f32 = 5.0;
const EPS: f32 = 1e-4;

fn pack_data() -> &'static ResourcePackData {
    static DATA: OnceLock<ResourcePackData> = OnceLock::new();
    DATA.get_or_init(|| {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let downloads = concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/downloads");
            let version =
                std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../mc-version"))
                    .expect("mc-version file")
                    .trim()
                    .to_string();
            let pack = std::path::Path::new(downloads).join(&version);
            assert!(
                pack.join("assets/minecraft/blockstates").is_dir(),
                "extracted MC assets missing at {} (build mcre_assets first)",
                pack.display()
            );

            let fs = NativeFs::new_absolute(downloads.into())
                .await
                .expect("native fs");
            let path = FsPath::from(version.as_str());
            let meta = path.join("pack.mcmeta");
            if !fs.exists(&meta).await {
                fs.write(
                    &meta,
                    br#"{"pack":{"description":"Minecraft (generated)"}}"#,
                )
                .await
                .expect("write pack.mcmeta");
            }

            let mut set = ResourcePackSet::new();
            set.add_pack(&fs, &path).await.expect("load default pack");
            set.build(&fs).await.expect("bake pack")
        })
    })
}

fn atlas() -> Atlas {
    Atlas {
        width: 16,
        height: 16,
        pixels: vec![],
        mips: Vec::new(),
        regions: vec![],
        has_alpha: vec![],
        animations: Vec::new(),
        fallback: AtlasRegion {
            x: 0,
            y: 0,
            width: 16,
            height: 16,
        },
    }
}

fn mesh_of(world: &World) -> Mesh {
    let chunk = world.chunk_at_pos(ChunkPos::new(0, 0)).unwrap();
    mesh_chunk(
        pack_data(),
        &atlas(),
        chunk,
        world,
        [0, 0],
        LightMode::Flat,
        MeshOptions::default(),
    )
}

fn quads(mesh: &Mesh) -> Vec<[[f32; 3]; 4]> {
    mesh.indices
        .chunks(6)
        .map(|quad| std::array::from_fn(|i| mesh.vertices[quad[i] as usize].position))
        .collect()
}

fn in_column(quad: &[[f32; 3]; 4]) -> bool {
    quad.iter().all(|v| {
        v[0] >= COLUMN_X - EPS
            && v[0] <= COLUMN_X + 1.0 + EPS
            && v[2] >= COLUMN_Z - EPS
            && v[2] <= COLUMN_Z + 1.0 + EPS
    })
}

fn column_quads(mesh: &Mesh) -> Vec<[[f32; 3]; 4]> {
    quads(mesh).into_iter().filter(in_column).collect()
}

fn planar_y(quads: &[[[f32; 3]; 4]], plane: f32) -> Vec<[[f32; 3]; 4]> {
    quads
        .iter()
        .filter(|quad| quad.iter().all(|v| (v[1] - plane).abs() < 5e-5))
        .copied()
        .collect()
}

fn normal(quad: &[[f32; 3]; 4]) -> [f32; 3] {
    let a = sub(quad[1], quad[0]);
    let b = sub(quad[2], quad[1]);
    let n = cross(a, b);
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    [n[0] / len, n[1] / len, n[2] / len]
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

fn world_with(blocks: &[(i32, i32, i32, BlockState)]) -> World {
    let mut world = World::new(1);
    let mut chunk = Chunk::new();
    for &(x, y, z, state) in blocks {
        chunk.set_block(x, y, z, state);
    }
    world.replace_chunk(chunk);
    world
}

fn stone() -> BlockState {
    Block::STONE.default_state()
}

fn powder_snow() -> BlockState {
    Block::POWDER_SNOW.default_state()
}

fn copper_grate() -> BlockState {
    Block::COPPER_GRATE.default_state()
}

#[test]
fn powder_snow_block_emits_its_shell_faces() {
    let world = world_with(&[(5, 10, 5, powder_snow())]);
    let mesh = mesh_of(&world);
    assert_eq!(quads(&mesh).len(), 12, "powder snow shell is 12 quads");
}

#[test]
fn powder_snow_stack_culls_shared_shell_faces() {
    let world = world_with(&[
        (5, 10, 5, powder_snow()),
        (5, 11, 5, powder_snow()),
        (8, 10, 8, powder_snow()),
    ]);
    let mesh = mesh_of(&world);
    assert_eq!(quads(&mesh).len(), 12 + 20, "isolated 12 + stack 20 quads");

    let column = column_quads(&mesh);
    assert_eq!(column.len(), 20, "4 shell faces culled between the pair");
    for quad in &column {
        assert!(
            !quad.iter().all(|v| (v[1] - 11.0).abs() < 5e-4),
            "shell face inside the shared boundary survived: {quad:?}"
        );
    }

    for plane in [10.0, 10.000125, 11.999875, 12.0] {
        assert_eq!(
            planar_y(&column, plane).len(),
            1,
            "outer shell plane {plane} should keep its face"
        );
    }
}

#[test]
fn powder_snow_does_not_hide_stone_face() {
    let world = world_with(&[(5, 10, 5, stone()), (5, 11, 5, powder_snow())]);
    let mesh = mesh_of(&world);
    assert_eq!(quads(&mesh).len(), 6 + 10, "stone cube + powder snow shell");

    let column = column_quads(&mesh);
    let at_boundary = planar_y(&column, 11.0);
    assert_eq!(at_boundary.len(), 1, "only the stone top face at y=11");
    assert!(
        normal(&at_boundary[0])[1] > 0.5,
        "stone top face must point up"
    );
    assert!(
        planar_y(&column, 11.000125).is_empty(),
        "powder snow inner bottom shell face culled by stone"
    );
}

#[test]
fn copper_grate_stack_culls_shared_face() {
    let world = world_with(&[(5, 10, 5, copper_grate()), (5, 11, 5, copper_grate())]);
    let mesh = mesh_of(&world);
    assert_eq!(quads(&mesh).len(), 10, "6 + 6 faces minus the shared pair");

    let column = column_quads(&mesh);
    assert_eq!(planar_y(&column, 10.0).len(), 1);
    assert_eq!(planar_y(&column, 12.0).len(), 1);
    assert!(
        planar_y(&column, 11.0).is_empty(),
        "shared grate face survived"
    );
}

#[test]
fn mixed_oxidation_grates_keep_shared_faces() {
    let world = world_with(&[
        (5, 10, 5, copper_grate()),
        (5, 11, 5, Block::EXPOSED_COPPER_GRATE.default_state()),
    ]);
    let mesh = mesh_of(&world);
    assert_eq!(quads(&mesh).len(), 12, "different grate blocks do not cull");

    let column = column_quads(&mesh);
    let at_boundary = planar_y(&column, 11.0);
    assert_eq!(at_boundary.len(), 2, "both shared faces present");
    assert!(at_boundary.iter().any(|q| normal(q)[1] > 0.5));
    assert!(at_boundary.iter().any(|q| normal(q)[1] < -0.5));
}

#[test]
fn copper_grate_does_not_hide_stone_face() {
    let world = world_with(&[(5, 10, 5, stone()), (5, 11, 5, copper_grate())]);
    let mesh = mesh_of(&world);
    assert_eq!(
        quads(&mesh).len(),
        6 + 5,
        "grate bottom face culled by stone"
    );

    let column = column_quads(&mesh);
    let at_boundary = planar_y(&column, 11.0);
    assert_eq!(at_boundary.len(), 1, "only the stone top face at y=11");
    assert!(normal(&at_boundary[0])[1] > 0.5);
}

#[test]
fn waterlogged_and_dry_grate_cull_shared_face() {
    let waterlogged = Block::COPPER_GRATE
        .state(&[mcre_world::PropVal::Waterlogged(true)])
        .expect("waterlogged grate");
    let world = world_with(&[(5, 10, 5, copper_grate()), (5, 11, 5, waterlogged)]);
    let mesh = mesh_of(&world);
    assert_eq!(
        quads(&mesh).len(),
        10,
        "same grate block culls across states"
    );
}

fn east_straight_stair() -> BlockState {
    (u16::from(Block::OAK_STAIRS.min_state())..=u16::from(Block::OAK_STAIRS.max_state()))
        .map(BlockState::from)
        .find(|state| {
            state.facing() == mcre_core::Direction::East
                && state.half() == mcre_world::Half::Bottom
                && state.stairs_shape() == mcre_world::StairsShape::Straight
        })
        .unwrap()
}

#[test]
fn partial_neighbors_keep_empty_shape_faces() {
    let fence = Block::OAK_FENCE.default_state();
    let stair = east_straight_stair();

    let fence_only = quads(&mesh_of(&world_with(&[(5, 10, 5, fence)]))).len();
    let stair_only = quads(&mesh_of(&world_with(&[(5, 10, 5, stair)]))).len();

    // A fence post's up face is not the unit-cube singleton, so powder snow
    // keeps its bottom shell and glass keeps its bottom face above it.
    let mesh = mesh_of(&world_with(&[(5, 10, 5, fence), (5, 11, 5, powder_snow())]));
    assert_eq!(quads(&mesh).len(), fence_only + 12);
    let at_boundary = planar_y(&column_quads(&mesh), 11.0);
    assert!(
        at_boundary.iter().any(|quad| normal(quad)[1] < -0.5),
        "powder snow bottom shell face missing over the fence post"
    );

    let mesh = mesh_of(&world_with(&[
        (5, 10, 5, fence),
        (5, 11, 5, Block::GLASS.default_state()),
    ]));
    assert_eq!(quads(&mesh).len(), fence_only + 6);

    // A stair's down face slab fills the footprint but has interior
    // coordinate planes: it is not the cube singleton either.
    let mesh = mesh_of(&world_with(&[(5, 10, 5, stair), (5, 11, 5, powder_snow())]));
    assert_eq!(quads(&mesh).len(), stair_only + 12);

    // A bottom slab below still culls the stone above it through its
    // unit-cube down face; powder snow above a bottom slab is untouched
    // (the slab's up face is empty).
    let slab = (u16::from(Block::OAK_SLAB.min_state())..=u16::from(Block::OAK_SLAB.max_state()))
        .map(BlockState::from)
        .find(|state| state.slab_type() == mcre_world::SlabType::Bottom)
        .unwrap();
    let mesh = mesh_of(&world_with(&[(5, 10, 5, slab), (5, 11, 5, powder_snow())]));
    let slab_only = quads(&mesh_of(&world_with(&[(5, 10, 5, slab)]))).len();
    assert_eq!(quads(&mesh).len(), slab_only + 12);

    let mesh = mesh_of(&world_with(&[(5, 10, 5, powder_snow()), (5, 11, 5, slab)]));
    assert_eq!(quads(&mesh).len() - slab_only, 10);
}
