use bytemuck::{Pod, Zeroable};
use mcre_anvil::nbt::Tag;
use mcre_core::{Axis, Direction, GrassColorModifier};
use mcre_resource_pack::{
    Colormaps, ResourcePackData, TintSource,
    colormap::{DRY_FOLIAGE_DEFAULT, FOLIAGE_DEFAULT, GRASS_DEFAULT, sample_colormap},
    lookup::{BlockRule, BlockVariant},
    tint_sources,
};
use mcre_world::shape::{face_boxes, face_shape_is_cube, is_covered};
use mcre_world::{
    Biome, Block, BlockState, ChestType, Chunk, DoubleBlockHalf, SECTIONS_PER_CHUNK, WORLD_MIN_Y,
    World,
};

use crate::atlas::{Atlas, AtlasRegion};
use crate::occlusion::{FULL_VISIBILITY, SectionVisibility, compute_section_visibility};
use crate::vertex::{
    FACING_COUNT, FACING_UNASSIGNED, QuadEncodeInput, TerrainPass, TerrainVertex, encode_quad,
    facing_of, facing_of_cullface, opposite_facing, pack_local_section_index,
};

const TINT_BLEND_RADIUS: i32 = 2;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 3],
    /// Raw `(block, sky)` light levels (0-15) sampled around the face; the
    /// shader looks them up in the lightmap texture per vertex.
    pub light: [f32; 2],
}

/// Lighting tier for emitted quads: `Smooth` is vanilla per-vertex light with
/// corner ambient occlusion, `Flat` is one sample per quad and no AO.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LightMode {
    #[default]
    Smooth,
    Flat,
}

/// Meshing toggles beyond vanilla defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshOptions {
    /// Emit liquid quads double-sided so underwater surfaces stay visible
    /// with back-face culling on.
    pub double_sided_liquids: bool,
    /// Fast-graphics leaves: cull leaf-to-leaf faces (`cutoutLeaves`).
    pub fast_leaves: bool,
}

impl Default for MeshOptions {
    fn default() -> Self {
        Self {
            double_sided_liquids: true,
            fast_leaves: false,
        }
    }
}

pub struct Mesh {
    pub vertices: Vec<Vertex>,
    /// Opaque quads (block models and lava fluids).
    pub indices: Vec<u32>,
    /// Translucent quads (water fluids), drawn in a blended pass after
    /// `indices`.
    pub translucent_indices: Vec<u32>,
    /// One entry per 6-index group in `indices`, in the same order.
    pub opaque_meta: Vec<QuadMeta>,
    /// One entry per 6-index group in `translucent_indices`, in the same
    /// order.
    pub translucent_meta: Vec<QuadMeta>,
    /// Per-section face-connectivity matrices for the occlusion-cull graph
    /// (Sodium `VisibilityEncoding`), one per 16³ section of the column.
    pub section_visibility: [SectionVisibility; SECTIONS_PER_CHUNK],
}

impl Default for Mesh {
    fn default() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
            translucent_indices: Vec::new(),
            opaque_meta: Vec::new(),
            translucent_meta: Vec::new(),
            section_visibility: [FULL_VISIBILITY; SECTIONS_PER_CHUNK],
        }
    }
}

impl Mesh {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty() && self.translucent_indices.is_empty()
    }
}

/// Per-quad classification captured at mesh time, consumed by
/// [`pack_chunk_sections`].
#[derive(Clone, Copy, Debug)]
pub struct QuadMeta {
    /// Sodium `ModelQuadFacing` ordinal (`vertex::FACING_*`).
    pub facing: u8,
    pub pass: TerrainPass,
    pub block_pos: [i32; 3],
    pub state: BlockState,
}

/// Per-quad sort key for translucent back-to-front ordering (Sodium
/// `ModelQuadNormalRelativeComparator`): world-space center + face normal.
#[derive(Clone, Copy, Debug)]
pub struct QuadSortData {
    pub center: [f32; 3],
    pub normal: [f32; 3],
}

/// Packed GPU vertices for one 16³ section, split into per-pass facing runs.
pub struct SectionPass {
    /// Contiguous runs for facings `0..7`; run `f` starts at
    /// `sum(counts[0..f])`.
    pub vertices: Vec<TerrainVertex>,
    pub counts: [u32; FACING_COUNT],
    /// Per-quad sort keys parallel to `vertices` quads (4 vertices each).
    /// Populated for the translucent pass only.
    pub sort: Vec<QuadSortData>,
}

impl SectionPass {
    fn is_empty(&self) -> bool {
        self.counts.iter().all(|&c| c == 0)
    }

    pub fn facing_offset(&self, facing: u8) -> u32 {
        self.counts[..facing as usize].iter().sum()
    }
}

/// Packed output for one chunk column: 24 sections × 3 passes.
pub struct SectionMesh {
    pub passes: [SectionPass; 3],
}

impl SectionMesh {
    fn is_empty(&self) -> bool {
        self.passes.iter().all(SectionPass::is_empty)
    }
}

/// Packs a stage-A [`Mesh`] into per-section facing runs of packed vertices.
/// Quads are assigned to the section that owns their block; overhanging
/// geometry stays in the owner (the 20-bit position range covers −8..+24
/// blocks around the section origin).
pub fn pack_chunk_sections(
    mesh: &Mesh,
    origin: [i32; 2],
) -> [Option<SectionMesh>; SECTIONS_PER_CHUNK] {
    let section_x = origin[0].div_euclid(16);
    let section_z = origin[1].div_euclid(16);
    let local_x = (section_x & 7) as u8;
    let local_z = (section_z & 7) as u8;

    let mut builders: Vec<[[Vec<TerrainVertex>; FACING_COUNT]; 3]> = (0..SECTIONS_PER_CHUNK)
        .map(|_| core::array::from_fn(|_| core::array::from_fn(|_| Vec::new())))
        .collect();
    let mut sort_builders: Vec<[Vec<QuadSortData>; FACING_COUNT]> = (0..SECTIONS_PER_CHUNK)
        .map(|_| core::array::from_fn(|_| Vec::new()))
        .collect();

    let mut pack_list = |indices: &[u32], metas: &[QuadMeta]| {
        for (group, meta) in indices.as_chunks::<6>().0.iter().zip(metas.iter()) {
            let quad = recover_quad(group);
            let rel_y = meta.block_pos[1] - WORLD_MIN_Y;
            let section_y = rel_y.div_euclid(16);
            if !(0..SECTIONS_PER_CHUNK as i32).contains(&section_y) {
                continue;
            }
            let section_origin = [
                origin[0] as f32,
                (WORLD_MIN_Y + section_y * 16) as f32,
                origin[1] as f32,
            ];
            let mut positions = [[0.0; 3]; 4];
            let mut uvs = [[0.0; 2]; 4];
            let mut colors = [[0.0; 3]; 4];
            let mut light = [[0.0_f32; 2]; 4];
            for (i, &vertex_index) in quad.iter().enumerate() {
                let vertex = &mesh.vertices[vertex_index as usize];
                positions[i] = [
                    vertex.position[0] - section_origin[0],
                    vertex.position[1] - section_origin[1],
                    vertex.position[2] - section_origin[2],
                ];
                uvs[i] = vertex.uv;
                colors[i] = vertex.color;
                light[i] = [vertex.light[0], vertex.light[1]];
            }
            let packed = encode_quad(&QuadEncodeInput {
                positions,
                uvs,
                colors,
                light,
                block_pos: meta.block_pos,
                state_id: u16::from(meta.state),
                translucent: meta.pass == TerrainPass::Translucent,
                emission: meta.state.light_emission().min(15),
                section_id: pack_local_section_index(local_x, (section_y as u8) & 0x3, local_z),
            });
            let facing = (meta.facing as usize).min(FACING_COUNT - 1);
            builders[section_y as usize][meta.pass.index()][facing].extend_from_slice(&packed);
            if meta.pass == TerrainPass::Translucent {
                let mut center = [0.0; 3];
                for position in &positions {
                    for axis in 0..3 {
                        center[axis] += position[axis];
                    }
                }
                for component in &mut center {
                    *component *= 0.25;
                }
                sort_builders[section_y as usize][facing].push(QuadSortData {
                    center: [
                        section_origin[0] + center[0],
                        section_origin[1] + center[1],
                        section_origin[2] + center[2],
                    ],
                    normal: crate::vertex::compute_face_normal(&positions),
                });
            }
        }
    };

    pack_list(&mesh.indices, &mesh.opaque_meta);
    pack_list(&mesh.translucent_indices, &mesh.translucent_meta);

    core::array::from_fn(|section| {
        let mut passes = core::array::from_fn(|_| SectionPass {
            vertices: Vec::new(),
            counts: [0; FACING_COUNT],
            sort: Vec::new(),
        });
        for pass in 0..3 {
            for (facing, run) in builders[section][pass].iter().enumerate() {
                passes[pass].counts[facing] = run.len() as u32;
                passes[pass].vertices.extend_from_slice(run);
                if pass == TerrainPass::Translucent.index() {
                    passes[pass]
                        .sort
                        .extend_from_slice(&sort_builders[section][facing]);
                }
            }
        }
        let mesh = SectionMesh { passes };
        (!mesh.is_empty()).then_some(mesh)
    })
}

/// Recovers the quad's vertex order `[v0, v1, v2, v3]` from one 6-index
/// triangle pair (front `[0,1,2, 0,2,3]` or reversed fluid copies).
fn recover_quad(group: &[u32]) -> [u32; 4] {
    let (a, b, c) = (group[0], group[1], group[2]);
    let fourth = [group[3], group[4], group[5]]
        .into_iter()
        .find(|&i| i != a && i != b && i != c)
        .expect("quad group has a unique fourth vertex");
    [a, b, c, fourth]
}

/// Neighbor block lookups by world coordinates, used for face culling across
/// chunk borders. Out-of-range positions read as air.
pub trait BlockAccess {
    fn block_at(&self, x: i32, y: i32, z: i32) -> BlockState;

    /// `(block, sky)` light levels at a position. Out-of-range positions read
    /// as no block light under open sky.
    fn light_at(&self, x: i32, y: i32, z: i32) -> (u8, u8) {
        let _ = (x, y, z);
        (0, 15)
    }

    /// Biome at a position for tint evaluation; unloaded positions read as
    /// plains.
    fn biome_at(&self, x: i32, y: i32, z: i32) -> Biome {
        let _ = (x, y, z);
        Biome::PLAINS
    }
}

impl BlockAccess for World {
    fn block_at(&self, x: i32, y: i32, z: i32) -> BlockState {
        self.get_block((x, y, z))
            .unwrap_or_else(|| Block::AIR.default_state())
    }

    fn light_at(&self, x: i32, y: i32, z: i32) -> (u8, u8) {
        self.light_level((x, y, z))
    }

    fn biome_at(&self, x: i32, y: i32, z: i32) -> Biome {
        self.biome_at((x, y, z))
    }
}

/// Meshes an entire chunk. Quads with a baked `cullface` are dropped when the
/// neighboring block hides that face (vanilla `Block.shouldRenderFace`,
/// full-cube tier); quads without a `cullface` are always emitted.
pub fn mesh_chunk<A: BlockAccess + ?Sized>(
    data: &ResourcePackData,
    atlas: &Atlas,
    chunk: &Chunk,
    access: &A,
    origin: [i32; 2],
    mode: LightMode,
    options: MeshOptions,
) -> Mesh {
    let mut mesh = Mesh::default();

    for (local_x, y, local_z, state) in chunk.iter_blocks() {
        let block_pos = [origin[0] + local_x, y, origin[1] + local_z];
        if let Some(fluid) = mcre_world::fluid_state(state) {
            crate::fluid::mesh_fluid(
                &mut mesh, data, atlas, access, block_pos, state, fluid, options,
            );
        }
        mesh_chest(data, atlas, access, &mut mesh, block_pos, state);
        mesh_sign_text(data, atlas, chunk, access, &mut mesh, block_pos, state);
        let Some(entry) = data.entries.get(u16::from(state) as usize) else {
            continue;
        };
        for_each_variant(entry, block_pos, |variant| {
            for quad in &variant.quads {
                if let Some(direction) = quad.cullface {
                    let delta = direction.delta();
                    let neighbor = access.block_at(
                        origin[0] + local_x + delta[0],
                        y + delta[1],
                        origin[1] + local_z + delta[2],
                    );
                    if !should_render_face(data, state, neighbor, direction, options) {
                        continue;
                    }
                }

                let region = atlas.region(quad.texture_id);
                let base_color = quad_color(data, access, state, block_pos, quad);

                // Vanilla `ModelBlockRenderer` dispatch: smooth AO only when the
                // model opts in, the block does not emit light, and the state
                // is not emissive-rendered.
                let (colors, lights) = if mode == LightMode::Smooth
                    && quad.ambient_occlusion
                    && quad.light_emission == 0
                    && !state.emissive_rendering()
                {
                    smooth_face(
                        access,
                        quad.direction,
                        block_pos,
                        &quad.vertices,
                        base_color,
                        state,
                    )
                } else {
                    flat_face(
                        access,
                        quad.direction,
                        block_pos,
                        &quad.vertices,
                        quad.cullface.is_some(),
                        state,
                        base_color,
                    )
                };

                let base = mesh.vertices.len() as u32;
                for i in 0..4 {
                    let v = &quad.vertices[i];
                    let uv = &quad.uvs[i];
                    let position = [
                        origin[0] as f32 + local_x as f32 + v[0] / 16.0,
                        y as f32 + v[1] / 16.0,
                        origin[1] as f32 + local_z as f32 + v[2] / 16.0,
                    ];
                    mesh.vertices.push(Vertex {
                        position,
                        uv: atlas_uv(region, atlas, uv),
                        color: colors[i],
                        light: lights[i],
                    });
                }

                let facing = facing_of_cullface(quad.cullface);
                let pass = if atlas.region_has_alpha(quad.texture_id) {
                    TerrainPass::Cutout
                } else {
                    TerrainPass::Solid
                };
                mesh.indices.extend_from_slice(&[
                    base,
                    base + 1,
                    base + 2,
                    base,
                    base + 2,
                    base + 3,
                ]);
                mesh.opaque_meta.push(QuadMeta {
                    facing,
                    pass,
                    block_pos,
                    state,
                });
                if options.double_sided_liquids && state.block().liquid_fluid().is_some() {
                    mesh.indices.extend_from_slice(&[
                        base + 2,
                        base + 1,
                        base,
                        base + 3,
                        base + 2,
                        base,
                    ]);
                    mesh.opaque_meta.push(QuadMeta {
                        facing: opposite_facing(facing),
                        pass,
                        block_pos,
                        state,
                    });
                }
            }
        });
    }

    mesh_entities(data, atlas, access, &mut mesh, chunk);
    mesh.section_visibility = compute_section_visibility(chunk);
    mesh
}

/// Vanilla chest special model (`ChestModel` single or double-half layer):
/// a bottom box, a lid, and the lock, oriented by the state's `facing` and
/// textured from `textures/entity/chest/*`.
fn mesh_chest<A: BlockAccess + ?Sized>(
    data: &ResourcePackData,
    atlas: &Atlas,
    access: &A,
    mesh: &mut Mesh,
    block_pos: [i32; 3],
    state: BlockState,
) {
    let (textures, ender) = match state.block() {
        Block::CHEST => (data.chest_textures.normal, false),
        Block::TRAPPED_CHEST => (data.chest_textures.trapped, false),
        Block::COPPER_CHEST | Block::WAXED_COPPER_CHEST => (data.chest_textures.copper, false),
        Block::EXPOSED_COPPER_CHEST | Block::WAXED_EXPOSED_COPPER_CHEST => {
            (data.chest_textures.copper_exposed, false)
        }
        Block::WEATHERED_COPPER_CHEST | Block::WAXED_WEATHERED_COPPER_CHEST => {
            (data.chest_textures.copper_weathered, false)
        }
        Block::OXIDIZED_COPPER_CHEST | Block::WAXED_OXIDIZED_COPPER_CHEST => {
            (data.chest_textures.copper_oxidized, false)
        }
        Block::ENDER_CHEST => (mcre_resource_pack::lookup::ChestTextures::default(), true),
        _ => return,
    };
    let chest_type = if ender {
        ChestType::Single
    } else {
        state.chest_type()
    };
    let (texture_id, hidden, half) = if ender {
        (data.chest_textures.ender, None, 0)
    } else {
        match chest_type {
            ChestType::Single => (textures.single, None, 0),
            ChestType::Left => (textures.left, Some(Direction::West), 1),
            ChestType::Right => (textures.right, Some(Direction::East), 2),
        }
    };
    let Some(path) = data.texture_table.get(texture_id as usize) else {
        return;
    };
    if !path.starts_with("minecraft:entity/chest/") {
        return;
    }

    let facing = state.facing();
    let region = atlas.region(texture_id);
    let pass = if atlas.region_has_alpha(texture_id) {
        TerrainPass::Cutout
    } else {
        TerrainPass::Solid
    };
    let (block_light, sky_light) = sample_light(access, block_pos);
    let light = [f32::from(block_light), f32::from(sky_light)];
    let boxes = match half {
        1 => [
            ([0.0, 0.0, 1.0, 15.0, 10.0, 14.0], [0.0, 19.0]),
            ([0.0, 9.0, 1.0, 15.0, 5.0, 14.0], [0.0, 0.0]),
            ([0.0, 7.0, 15.0, 1.0, 4.0, 1.0], [0.0, 0.0]),
        ],
        2 => [
            ([1.0, 0.0, 1.0, 15.0, 10.0, 14.0], [0.0, 19.0]),
            ([1.0, 9.0, 1.0, 15.0, 5.0, 14.0], [0.0, 0.0]),
            ([15.0, 7.0, 15.0, 1.0, 4.0, 1.0], [0.0, 0.0]),
        ],
        _ => [
            ([1.0, 0.0, 1.0, 14.0, 10.0, 14.0], [0.0, 19.0]),
            ([1.0, 9.0, 1.0, 14.0, 5.0, 14.0], [0.0, 0.0]),
            ([7.0, 7.0, 15.0, 2.0, 4.0, 1.0], [0.0, 0.0]),
        ],
    };
    for (cube, texture_origin) in boxes {
        emit_chest_box(
            atlas,
            mesh,
            block_pos,
            state,
            facing,
            hidden,
            region,
            pass,
            light,
            cube,
            texture_origin,
        );
    }
}

type ModelBoxFace = (Direction, [[f32; 3]; 4], [[f32; 2]; 4]);

/// Vanilla `ModelPart.Cube` face layout: vertices and texture coordinates
/// for a box `[min_x, min_y, min_z, width, height, depth]` at `texture_origin`.
/// `growth` expands the geometry on every side while the texture layout keeps
/// using the original dimensions, matching `CubeDeformation`.
fn model_box_faces(
    cube: [f32; 6],
    texture_origin: [f32; 2],
    mirror: bool,
    growth: f32,
) -> [ModelBoxFace; 6] {
    let [x, y, z, width, height, depth] = cube;
    let [tex_u, tex_v] = texture_origin;
    let (min_x, max_x) = if mirror {
        (x + width + growth, x - growth)
    } else {
        (x - growth, x + width + growth)
    };
    let min_y = y - growth;
    let max_y = y + height + growth;
    let min_z = z - growth;
    let max_z = z + depth + growth;
    let points = [
        [min_x, min_y, min_z],
        [max_x, min_y, min_z],
        [max_x, max_y, min_z],
        [min_x, max_y, min_z],
        [min_x, min_y, max_z],
        [max_x, min_y, max_z],
        [max_x, max_y, max_z],
        [min_x, max_y, max_z],
    ];
    let u0 = tex_u;
    let u1 = u0 + depth;
    let u2 = u1 + width;
    let u22 = u2 + width;
    let u3 = u2 + depth;
    let u4 = u3 + width;
    let v0 = tex_v;
    let v1 = v0 + depth;
    let v2 = v1 + height;
    let mut faces = [
        (
            Direction::Down,
            [points[5], points[4], points[0], points[1]],
            [[u2, v0], [u1, v0], [u1, v1], [u2, v1]],
        ),
        (
            Direction::Up,
            [points[2], points[3], points[7], points[6]],
            [[u22, v1], [u2, v1], [u2, v0], [u22, v0]],
        ),
        (
            Direction::West,
            [points[0], points[4], points[7], points[3]],
            [[u1, v1], [u0, v1], [u0, v2], [u1, v2]],
        ),
        (
            Direction::North,
            [points[1], points[0], points[3], points[2]],
            [[u2, v1], [u1, v1], [u1, v2], [u2, v2]],
        ),
        (
            Direction::East,
            [points[5], points[1], points[2], points[6]],
            [[u3, v1], [u2, v1], [u2, v2], [u3, v2]],
        ),
        (
            Direction::South,
            [points[4], points[5], points[6], points[7]],
            [[u4, v1], [u3, v1], [u3, v2], [u4, v2]],
        ),
    ];
    if mirror {
        for (_, vertices, uvs) in &mut faces {
            vertices.reverse();
            uvs.reverse();
        }
    }
    faces
}

#[allow(clippy::too_many_arguments)]
fn emit_chest_box(
    atlas: &Atlas,
    mesh: &mut Mesh,
    block_pos: [i32; 3],
    state: BlockState,
    facing: Direction,
    hidden: Option<Direction>,
    region: AtlasRegion,
    pass: TerrainPass,
    light: [f32; 2],
    cube: [f32; 6],
    texture_origin: [f32; 2],
) {
    for (local_direction, local_vertices, uvs) in model_box_faces(cube, texture_origin, false, 0.0)
    {
        if hidden == Some(local_direction) {
            continue;
        }
        let direction = rotate_chest_direction(local_direction, facing);
        let vertices = local_vertices.map(|vertex| rotate_chest_position(vertex, facing));
        let base = mesh.vertices.len() as u32;
        for index in 0..4 {
            mesh.vertices.push(Vertex {
                position: [
                    block_pos[0] as f32 + vertices[index][0] / 16.0,
                    block_pos[1] as f32 + vertices[index][1] / 16.0,
                    block_pos[2] as f32 + vertices[index][2] / 16.0,
                ],
                uv: atlas_uv_dimensions(region, atlas, &uvs[index], [64.0, 64.0]),
                color: [1.0; 3],
                light,
            });
        }
        mesh.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        mesh.opaque_meta.push(QuadMeta {
            facing: facing_of(direction),
            pass,
            block_pos,
            state,
        });
    }
}

fn rotate_chest_position(position: [f32; 3], facing: Direction) -> [f32; 3] {
    match facing {
        Direction::South => position,
        Direction::West => [16.0 - position[2], position[1], position[0]],
        Direction::North => [16.0 - position[0], position[1], 16.0 - position[2]],
        Direction::East => [position[2], position[1], 16.0 - position[0]],
        Direction::Down | Direction::Up => position,
    }
}

fn rotate_chest_direction(direction: Direction, facing: Direction) -> Direction {
    let turns = match facing {
        Direction::South => 0,
        Direction::West => 1,
        Direction::North => 2,
        Direction::East => 3,
        Direction::Down | Direction::Up => 0,
    };
    let mut direction = direction;
    for _ in 0..turns {
        direction = match direction {
            Direction::North => Direction::East,
            Direction::East => Direction::South,
            Direction::South => Direction::West,
            Direction::West => Direction::North,
            Direction::Down | Direction::Up => direction,
        };
    }
    direction
}

const SIGN_TEXT_LINE_HEIGHT: f32 = 10.0;
const SIGN_TEXT_MAX_LINES: usize = 4;
const SIGN_TEXT_SCALE: f32 = 1.0 / 96.0;
const SIGN_TEXT_OFFSET: [f32; 3] = [0.0, 0.333_333_34, 0.046_666_667];
const SIGN_TEXT_WALL_OFFSET: [f32; 3] = [0.0, -0.3125, -0.4375];
const SIGN_TEXT_DEPTH_OFFSET: f32 = 0.001;

/// Vanilla standing/wall sign text (`AbstractSignRenderer` +
/// `StandingSignRenderer`): four centred lines of ASCII glyphs from
/// `font/ascii.png`, transformed into the sign's front or back plane.
fn mesh_sign_text<A: BlockAccess + ?Sized>(
    data: &ResourcePackData,
    atlas: &Atlas,
    chunk: &Chunk,
    access: &A,
    mesh: &mut Mesh,
    block_pos: [i32; 3],
    state: BlockState,
) {
    let name = state.block().name();
    if !name.ends_with("_sign") || name.contains("hanging_sign") {
        return;
    }
    let Some(block_entity) = chunk.block_entity_at((block_pos[0], block_pos[1], block_pos[2]))
    else {
        return;
    };
    let Some(mcre_anvil::BlockEntityPayload::Sign(sign)) =
        mcre_anvil::decode_block_entity_payload(block_entity)
    else {
        return;
    };
    let wall = name.ends_with("_wall_sign");
    let angle = if wall {
        direction_y_rotation(state.facing())
    } else {
        f32::from(state.rotation()) * 22.5
    };
    let font_id = data.font_textures.ascii;
    let Some(path) = data.texture_table.get(font_id as usize) else {
        return;
    };
    if !path.starts_with("minecraft:font/") {
        return;
    }
    let region = atlas.region(font_id);
    let (block_light, sky_light) = sample_light(access, block_pos);
    let block_light = [f32::from(block_light), f32::from(sky_light)];

    for (text, back) in [(&sign.front, false), (&sign.back, true)] {
        if text.lines.iter().all(|line| line.is_empty()) {
            continue;
        }
        let light = if text.glowing {
            [15.0, 15.0]
        } else {
            block_light
        };
        let color = sign_text_color(&text.color, text.glowing);
        for (line_index, line) in text.lines.iter().take(SIGN_TEXT_MAX_LINES).enumerate() {
            emit_sign_line(
                atlas, mesh, block_pos, state, angle, wall, back, region, line_index, line, color,
                light,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_sign_line(
    atlas: &Atlas,
    mesh: &mut Mesh,
    block_pos: [i32; 3],
    state: BlockState,
    angle: f32,
    wall: bool,
    back: bool,
    region: AtlasRegion,
    line_index: usize,
    line: &str,
    color: [f32; 3],
    light: [f32; 2],
) {
    let transform = sign_text_transform(angle, wall, back);
    let width: f32 = visible_sign_chars(line)
        .map(|character| sign_glyph(atlas, region, character).map_or(0.0, |(_, advance)| advance))
        .sum();
    let mut x = -width / 2.0;
    let y = line_index as f32 * SIGN_TEXT_LINE_HEIGHT - 2.0 * SIGN_TEXT_LINE_HEIGHT;
    for character in visible_sign_chars(line) {
        let Some((uvs, advance)) = sign_glyph(atlas, region, character) else {
            continue;
        };
        if !character.is_whitespace() {
            let corners = [[x, y], [x, y + 8.0], [x + 8.0, y + 8.0], [x + 8.0, y]];
            let base = mesh.vertices.len() as u32;
            for (index, corner) in corners.iter().enumerate() {
                let position = transform(*corner);
                mesh.vertices.push(Vertex {
                    position: [
                        block_pos[0] as f32 + position[0],
                        block_pos[1] as f32 + position[1],
                        block_pos[2] as f32 + position[2],
                    ],
                    uv: uvs[index],
                    color,
                    light,
                });
            }
            mesh.translucent_indices.extend_from_slice(&[
                base,
                base + 1,
                base + 2,
                base,
                base + 2,
                base + 3,
            ]);
            mesh.translucent_meta.push(QuadMeta {
                facing: FACING_UNASSIGNED,
                pass: TerrainPass::Translucent,
                block_pos,
                state,
            });
        }
        x += advance;
    }
}

/// Glyph UV quad (top-left, bottom-left, bottom-right, top-right) and advance
/// for one ASCII character, derived from `font/ascii.png` the way vanilla's
/// `BitmapProvider` measures the rightmost opaque column.
fn sign_glyph(atlas: &Atlas, region: AtlasRegion, character: char) -> Option<([[f32; 2]; 4], f32)> {
    let mut code = u32::from(character);
    if !(0x20..=0xFF).contains(&code) {
        code = u32::from('?');
    }
    let index = code - 0x20;
    let columns = 16;
    let rows = 16;
    let cell_width = region.width / columns;
    let cell_height = region.height / rows;
    if cell_width == 0 || cell_height == 0 {
        return None;
    }
    let column = index % columns;
    let row = 2 + index / columns;
    if row >= rows {
        return None;
    }
    let x0 = region.x + column * cell_width;
    let y0 = region.y + row * cell_height;
    let mut actual_width = 0;
    for width in (0..cell_width).rev() {
        let mut opaque = false;
        for y in 0..cell_height {
            let pixel = ((y0 + y) * atlas.width + x0 + width) as usize * 4;
            if atlas.pixels.get(pixel + 3).is_some_and(|alpha| *alpha != 0) {
                opaque = true;
                break;
            }
        }
        if opaque {
            actual_width = width + 1;
            break;
        }
    }
    let scale = 8.0 / cell_height as f32;
    let advance = if character == ' ' {
        4.0
    } else {
        ((0.5 + actual_width as f32 * scale) as u32 + 1) as f32
    };
    let size = [region.width as f32, region.height as f32];
    let local_x = column * cell_width;
    let local_y = row * cell_height;
    let uv = |x: u32, y: u32| atlas_uv_dimensions(region, atlas, &[x as f32, y as f32], size);
    let uvs = [
        uv(local_x, local_y),
        uv(local_x, local_y + cell_height),
        uv(local_x + cell_width, local_y + cell_height),
        uv(local_x + cell_width, local_y),
    ];
    Some((uvs, advance))
}

/// `StandingSignRenderer.textTransformation`: block centre, yaw, optional
/// wall offset, optional 180° back flip, then text offset and 1/96 scale.
fn sign_text_transform(angle: f32, wall: bool, back: bool) -> impl Fn([f32; 2]) -> [f32; 3] {
    let (sin, cos) = angle.to_radians().sin_cos();
    let wall_offset = if wall {
        SIGN_TEXT_WALL_OFFSET
    } else {
        [0.0; 3]
    };
    let normal = if back { -1.0 } else { 1.0 };
    let normal_x = -normal * sin;
    let normal_z = normal * cos;
    move |corner| {
        let mut point = [
            corner[0] * SIGN_TEXT_SCALE + SIGN_TEXT_OFFSET[0] + wall_offset[0],
            -corner[1] * SIGN_TEXT_SCALE + SIGN_TEXT_OFFSET[1] + wall_offset[1],
            SIGN_TEXT_OFFSET[2] + wall_offset[2],
        ];
        if back {
            point[0] = -point[0];
            point[2] = -point[2];
        }
        let x = point[0] * cos - point[2] * sin;
        let z = point[0] * sin + point[2] * cos;
        [
            0.5 + x + normal_x * SIGN_TEXT_DEPTH_OFFSET,
            0.5 + point[1],
            0.5 + z + normal_z * SIGN_TEXT_DEPTH_OFFSET,
        ]
    }
}

/// Yields printable characters, dropping vanilla `§` formatting codes.
fn visible_sign_chars(line: &str) -> impl Iterator<Item = char> + '_ {
    let mut chars = line.chars();
    std::iter::from_fn(move || {
        loop {
            let character = chars.next()?;
            if character == '§' {
                chars.next();
                continue;
            }
            return Some(character);
        }
    })
}

fn direction_y_rotation(direction: Direction) -> f32 {
    match direction {
        Direction::South => 0.0,
        Direction::West => 90.0,
        Direction::North => 180.0,
        Direction::East => 270.0,
        Direction::Down | Direction::Up => 0.0,
    }
}

#[derive(Clone, Copy)]
struct EntityCube {
    offset: [f32; 3],
    rotation_x: f32,
    texture_origin: [f32; 2],
    cube: [f32; 6],
    growth: f32,
    mirror: bool,
}

#[derive(Clone, Copy)]
struct EntityCubeRot {
    offset: [f32; 3],
    rotation: [f32; 3],
    texture_origin: [f32; 2],
    cube: [f32; 6],
    growth: f32,
    mirror: bool,
}

struct EntityModel {
    texture_size: [f32; 2],
    cubes: &'static [EntityCube],
    rotated_cubes: &'static [EntityCubeRot],
}

const PIG_CUBES: &[EntityCube] = &[
    EntityCube {
        offset: [0.0, 12.0, -6.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 0.0],
        cube: [-4.0, -4.0, -8.0, 8.0, 8.0, 8.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 12.0, -6.0],
        rotation_x: 0.0,
        texture_origin: [16.0, 16.0],
        cube: [-2.0, 0.0, -9.0, 4.0, 3.0, 1.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 11.0, 2.0],
        rotation_x: std::f32::consts::FRAC_PI_2,
        texture_origin: [28.0, 8.0],
        cube: [-5.0, -10.0, -7.0, 10.0, 16.0, 8.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [-3.0, 18.0, 7.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 16.0],
        cube: [-2.0, 0.0, -2.0, 4.0, 6.0, 4.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [3.0, 18.0, 7.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 16.0],
        cube: [-2.0, 0.0, -2.0, 4.0, 6.0, 4.0],
        growth: 0.0,
        mirror: true,
    },
    EntityCube {
        offset: [-3.0, 18.0, -5.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 16.0],
        cube: [-2.0, 0.0, -2.0, 4.0, 6.0, 4.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [3.0, 18.0, -5.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 16.0],
        cube: [-2.0, 0.0, -2.0, 4.0, 6.0, 4.0],
        growth: 0.0,
        mirror: true,
    },
];

const COW_CUBES: &[EntityCube] = &[
    EntityCube {
        offset: [0.0, 4.0, -8.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 0.0],
        cube: [-4.0, -4.0, -6.0, 8.0, 8.0, 6.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 4.0, -8.0],
        rotation_x: 0.0,
        texture_origin: [1.0, 33.0],
        cube: [-3.0, 1.0, -7.0, 6.0, 3.0, 1.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 4.0, -8.0],
        rotation_x: 0.0,
        texture_origin: [22.0, 0.0],
        cube: [-5.0, -5.0, -5.0, 1.0, 3.0, 1.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 4.0, -8.0],
        rotation_x: 0.0,
        texture_origin: [22.0, 0.0],
        cube: [4.0, -5.0, -5.0, 1.0, 3.0, 1.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 5.0, 2.0],
        rotation_x: std::f32::consts::FRAC_PI_2,
        texture_origin: [18.0, 4.0],
        cube: [-6.0, -10.0, -7.0, 12.0, 18.0, 10.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 5.0, 2.0],
        rotation_x: std::f32::consts::FRAC_PI_2,
        texture_origin: [52.0, 0.0],
        cube: [-2.0, 2.0, -8.0, 4.0, 6.0, 1.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [-4.0, 12.0, 7.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 16.0],
        cube: [-2.0, 0.0, -2.0, 4.0, 12.0, 4.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [4.0, 12.0, 7.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 16.0],
        cube: [-2.0, 0.0, -2.0, 4.0, 12.0, 4.0],
        growth: 0.0,
        mirror: true,
    },
    EntityCube {
        offset: [-4.0, 12.0, -5.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 16.0],
        cube: [-2.0, 0.0, -2.0, 4.0, 12.0, 4.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [4.0, 12.0, -5.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 16.0],
        cube: [-2.0, 0.0, -2.0, 4.0, 12.0, 4.0],
        growth: 0.0,
        mirror: true,
    },
];

const CHICKEN_CUBES: &[EntityCube] = &[
    EntityCube {
        offset: [0.0, 15.0, -4.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 0.0],
        cube: [-2.0, -6.0, -2.0, 4.0, 6.0, 3.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 15.0, -4.0],
        rotation_x: 0.0,
        texture_origin: [14.0, 0.0],
        cube: [-2.0, -4.0, -4.0, 4.0, 2.0, 2.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 15.0, -4.0],
        rotation_x: 0.0,
        texture_origin: [14.0, 4.0],
        cube: [-1.0, -2.0, -3.0, 2.0, 2.0, 2.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 16.0, 0.0],
        rotation_x: std::f32::consts::FRAC_PI_2,
        texture_origin: [0.0, 9.0],
        cube: [-3.0, -4.0, -3.0, 6.0, 8.0, 6.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [-2.0, 19.0, 1.0],
        rotation_x: 0.0,
        texture_origin: [26.0, 0.0],
        cube: [-1.0, 0.0, -3.0, 3.0, 5.0, 3.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [1.0, 19.0, 1.0],
        rotation_x: 0.0,
        texture_origin: [26.0, 0.0],
        cube: [-1.0, 0.0, -3.0, 3.0, 5.0, 3.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [-4.0, 13.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [24.0, 13.0],
        cube: [0.0, 0.0, -3.0, 1.0, 4.0, 6.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [4.0, 13.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [24.0, 13.0],
        cube: [-1.0, 0.0, -3.0, 1.0, 4.0, 6.0],
        growth: 0.0,
        mirror: false,
    },
];

const CREEPER_CUBES: &[EntityCube] = &[
    EntityCube {
        offset: [0.0, 6.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 0.0],
        cube: [-4.0, -8.0, -4.0, 8.0, 8.0, 8.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 6.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [16.0, 16.0],
        cube: [-4.0, 0.0, -2.0, 8.0, 12.0, 4.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [-2.0, 18.0, 4.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 16.0],
        cube: [-2.0, 0.0, -2.0, 4.0, 6.0, 4.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [2.0, 18.0, 4.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 16.0],
        cube: [-2.0, 0.0, -2.0, 4.0, 6.0, 4.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [-2.0, 18.0, -4.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 16.0],
        cube: [-2.0, 0.0, -2.0, 4.0, 6.0, 4.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [2.0, 18.0, -4.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 16.0],
        cube: [-2.0, 0.0, -2.0, 4.0, 6.0, 4.0],
        growth: 0.0,
        mirror: false,
    },
];

const SHEEP_CUBES: &[EntityCube] = &[
    EntityCube {
        offset: [0.0, 6.0, -8.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 0.0],
        cube: [-3.0, -4.0, -6.0, 6.0, 6.0, 8.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 5.0, 2.0],
        rotation_x: std::f32::consts::FRAC_PI_2,
        texture_origin: [28.0, 8.0],
        cube: [-4.0, -10.0, -7.0, 8.0, 16.0, 6.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [-3.0, 12.0, 7.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 16.0],
        cube: [-2.0, 0.0, -2.0, 4.0, 12.0, 4.0],
        growth: 0.0,
        mirror: true,
    },
    EntityCube {
        offset: [3.0, 12.0, 7.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 16.0],
        cube: [-2.0, 0.0, -2.0, 4.0, 12.0, 4.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [-3.0, 12.0, -5.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 16.0],
        cube: [-2.0, 0.0, -2.0, 4.0, 12.0, 4.0],
        growth: 0.0,
        mirror: true,
    },
    EntityCube {
        offset: [3.0, 12.0, -5.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 16.0],
        cube: [-2.0, 0.0, -2.0, 4.0, 12.0, 4.0],
        growth: 0.0,
        mirror: false,
    },
];

const SHEEP_WOOL_CUBES: &[EntityCube] = &[
    EntityCube {
        offset: [0.0, 6.0, -8.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 0.0],
        cube: [-3.0, -4.0, -4.0, 6.0, 6.0, 6.0],
        growth: 0.6,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 5.0, 2.0],
        rotation_x: std::f32::consts::FRAC_PI_2,
        texture_origin: [28.0, 8.0],
        cube: [-4.0, -10.0, -7.0, 8.0, 16.0, 6.0],
        growth: 1.75,
        mirror: false,
    },
    EntityCube {
        offset: [-3.0, 12.0, 7.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 16.0],
        cube: [-2.0, 0.0, -2.0, 4.0, 6.0, 4.0],
        growth: 0.5,
        mirror: false,
    },
    EntityCube {
        offset: [3.0, 12.0, 7.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 16.0],
        cube: [-2.0, 0.0, -2.0, 4.0, 6.0, 4.0],
        growth: 0.5,
        mirror: false,
    },
    EntityCube {
        offset: [-3.0, 12.0, -5.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 16.0],
        cube: [-2.0, 0.0, -2.0, 4.0, 6.0, 4.0],
        growth: 0.5,
        mirror: false,
    },
    EntityCube {
        offset: [3.0, 12.0, -5.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 16.0],
        cube: [-2.0, 0.0, -2.0, 4.0, 6.0, 4.0],
        growth: 0.5,
        mirror: false,
    },
];

const ZOMBIE_CUBES: &[EntityCube] = &[
    EntityCube {
        offset: [0.0, 0.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 0.0],
        cube: [-4.0, -8.0, -4.0, 8.0, 8.0, 8.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 0.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [32.0, 0.0],
        cube: [-4.0, -8.0, -4.0, 8.0, 8.0, 8.0],
        growth: 0.5,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 0.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [16.0, 16.0],
        cube: [-4.0, 0.0, -2.0, 8.0, 12.0, 4.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [-5.0, 2.0, 0.0],
        rotation_x: -1.3962634,
        texture_origin: [40.0, 16.0],
        cube: [-3.0, -2.0, -2.0, 4.0, 12.0, 4.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [5.0, 2.0, 0.0],
        rotation_x: -1.3962634,
        texture_origin: [40.0, 16.0],
        cube: [-1.0, -2.0, -2.0, 4.0, 12.0, 4.0],
        growth: 0.0,
        mirror: true,
    },
    EntityCube {
        offset: [-1.9, 12.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 16.0],
        cube: [-2.0, 0.0, -2.0, 4.0, 12.0, 4.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [1.9, 12.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 16.0],
        cube: [-2.0, 0.0, -2.0, 4.0, 12.0, 4.0],
        growth: 0.0,
        mirror: true,
    },
];

const SKELETON_CUBES: &[EntityCube] = &[
    EntityCube {
        offset: [0.0, 0.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 0.0],
        cube: [-4.0, -8.0, -4.0, 8.0, 8.0, 8.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 0.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [32.0, 0.0],
        cube: [-4.0, -8.0, -4.0, 8.0, 8.0, 8.0],
        growth: 0.5,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 0.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [16.0, 16.0],
        cube: [-4.0, 0.0, -2.0, 8.0, 12.0, 4.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [-5.0, 2.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [40.0, 16.0],
        cube: [-1.0, -2.0, -1.0, 2.0, 12.0, 2.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [5.0, 2.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [40.0, 16.0],
        cube: [-1.0, -2.0, -1.0, 2.0, 12.0, 2.0],
        growth: 0.0,
        mirror: true,
    },
    EntityCube {
        offset: [-2.0, 12.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 16.0],
        cube: [-1.0, 0.0, -1.0, 2.0, 12.0, 2.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [2.0, 12.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 16.0],
        cube: [-1.0, 0.0, -1.0, 2.0, 12.0, 2.0],
        growth: 0.0,
        mirror: true,
    },
];

const WOLF_CUBES: &[EntityCube] = &[
    EntityCube {
        offset: [-1.0, 13.5, -7.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 0.0],
        cube: [-2.0, -3.0, -2.0, 6.0, 6.0, 4.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [-1.0, 13.5, -7.0],
        rotation_x: 0.0,
        texture_origin: [16.0, 14.0],
        cube: [-2.0, -5.0, 0.0, 2.0, 2.0, 1.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [-1.0, 13.5, -7.0],
        rotation_x: 0.0,
        texture_origin: [16.0, 14.0],
        cube: [2.0, -5.0, 0.0, 2.0, 2.0, 1.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [-1.0, 13.5, -7.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 10.0],
        cube: [-0.5, -0.001, -5.0, 3.0, 3.0, 4.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 14.0, 2.0],
        rotation_x: std::f32::consts::FRAC_PI_2,
        texture_origin: [18.0, 14.0],
        cube: [-3.0, -2.0, -3.0, 6.0, 9.0, 6.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [-1.0, 14.0, -3.0],
        rotation_x: std::f32::consts::FRAC_PI_2,
        texture_origin: [21.0, 0.0],
        cube: [-3.0, -3.0, -3.0, 8.0, 6.0, 7.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [-2.5, 16.0, 7.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 18.0],
        cube: [0.0, 0.0, -1.0, 2.0, 8.0, 2.0],
        growth: 0.0,
        mirror: true,
    },
    EntityCube {
        offset: [0.5, 16.0, 7.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 18.0],
        cube: [0.0, 0.0, -1.0, 2.0, 8.0, 2.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [-2.5, 16.0, -4.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 18.0],
        cube: [0.0, 0.0, -1.0, 2.0, 8.0, 2.0],
        growth: 0.0,
        mirror: true,
    },
    EntityCube {
        offset: [0.5, 16.0, -4.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 18.0],
        cube: [0.0, 0.0, -1.0, 2.0, 8.0, 2.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [-1.0, 12.0, 8.0],
        rotation_x: std::f32::consts::PI / 5.0,
        texture_origin: [9.0, 18.0],
        cube: [0.0, 0.0, -1.0, 2.0, 8.0, 2.0],
        growth: 0.0,
        mirror: false,
    },
];

const PARROT_CUBES: &[EntityCube] = &[
    EntityCube {
        offset: [0.0, 16.5, -3.0],
        rotation_x: 0.4937,
        texture_origin: [2.0, 8.0],
        cube: [-1.5, 0.0, -1.5, 3.0, 6.0, 3.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 21.07, 1.16],
        rotation_x: 1.015,
        texture_origin: [22.0, 1.0],
        cube: [-1.5, -1.0, -1.0, 3.0, 4.0, 1.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [1.5, 16.94, -2.76],
        rotation_x: -0.6981,
        texture_origin: [19.0, 8.0],
        cube: [-0.5, 0.0, -1.5, 1.0, 5.0, 3.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [-1.5, 16.94, -2.76],
        rotation_x: -0.6981,
        texture_origin: [19.0, 8.0],
        cube: [-0.5, 0.0, -1.5, 1.0, 5.0, 3.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 15.69, -2.76],
        rotation_x: 0.0,
        texture_origin: [2.0, 2.0],
        cube: [-1.0, -1.5, -1.0, 2.0, 3.0, 2.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 13.69, -3.76],
        rotation_x: 0.0,
        texture_origin: [10.0, 0.0],
        cube: [-1.0, -0.5, -2.0, 2.0, 1.0, 4.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 15.19, -4.26],
        rotation_x: 0.0,
        texture_origin: [11.0, 7.0],
        cube: [-0.5, -1.0, -0.5, 1.0, 2.0, 1.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 13.94, -5.21],
        rotation_x: 0.0,
        texture_origin: [16.0, 7.0],
        cube: [-0.5, 0.0, -0.5, 1.0, 2.0, 1.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 13.54, -2.61],
        rotation_x: -0.2214,
        texture_origin: [2.0, 18.0],
        cube: [0.0, -4.0, -2.0, 0.0, 5.0, 4.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [1.0, 22.0, -1.05],
        rotation_x: -0.0299,
        texture_origin: [14.0, 18.0],
        cube: [-0.5, 0.0, -0.5, 1.0, 2.0, 1.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [-1.0, 22.0, -1.05],
        rotation_x: -0.0299,
        texture_origin: [14.0, 18.0],
        cube: [-0.5, 0.0, -0.5, 1.0, 2.0, 1.0],
        growth: 0.0,
        mirror: false,
    },
];

const BAT_CUBES: &[EntityCube] = &[
    EntityCube {
        offset: [0.0, 17.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 0.0],
        cube: [-1.5, 0.0, -1.0, 3.0, 5.0, 2.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 17.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 7.0],
        cube: [-2.0, -3.0, -1.0, 4.0, 3.0, 2.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [-1.5, 15.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [1.0, 15.0],
        cube: [-2.5, -4.0, 0.0, 3.0, 5.0, 0.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [1.1, 14.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [8.0, 15.0],
        cube: [-0.1, -3.0, 0.0, 3.0, 5.0, 0.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [-1.5, 17.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [12.0, 0.0],
        cube: [-2.0, -2.0, 0.0, 2.0, 7.0, 0.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [-3.5, 17.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [16.0, 0.0],
        cube: [-6.0, -2.0, 0.0, 6.0, 8.0, 0.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [1.5, 17.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [12.0, 7.0],
        cube: [0.0, -2.0, 0.0, 2.0, 7.0, 0.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [3.5, 17.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [16.0, 8.0],
        cube: [0.0, -2.0, 0.0, 6.0, 8.0, 0.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 22.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [16.0, 16.0],
        cube: [-1.5, 0.0, 0.0, 3.0, 2.0, 0.0],
        growth: 0.0,
        mirror: false,
    },
];

const BEE_CUBES: &[EntityCube] = &[
    EntityCube {
        offset: [0.0, 19.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 0.0],
        cube: [-3.5, -4.0, -5.0, 7.0, 7.0, 10.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 19.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [26.0, 7.0],
        cube: [0.0, -1.0, 5.0, 0.0, 1.0, 2.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 17.0, -5.0],
        rotation_x: 0.0,
        texture_origin: [2.0, 0.0],
        cube: [1.5, -2.0, -3.0, 1.0, 2.0, 3.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 17.0, -5.0],
        rotation_x: 0.0,
        texture_origin: [2.0, 3.0],
        cube: [-2.5, -2.0, -3.0, 1.0, 2.0, 3.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [-1.5, 15.0, -3.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 18.0],
        cube: [-9.0, 0.0, 0.0, 9.0, 0.0, 6.0],
        growth: 0.001,
        mirror: false,
    },
    EntityCube {
        offset: [1.5, 15.0, -3.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 18.0],
        cube: [0.0, 0.0, 0.0, 9.0, 0.0, 6.0],
        growth: 0.001,
        mirror: true,
    },
    EntityCube {
        offset: [1.5, 22.0, -2.0],
        rotation_x: 0.0,
        texture_origin: [26.0, 1.0],
        cube: [-5.0, 0.0, 0.0, 7.0, 2.0, 0.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [1.5, 22.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [26.0, 3.0],
        cube: [-5.0, 0.0, 0.0, 7.0, 2.0, 0.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [1.5, 22.0, 2.0],
        rotation_x: 0.0,
        texture_origin: [26.0, 5.0],
        cube: [-5.0, 0.0, 0.0, 7.0, 2.0, 0.0],
        growth: 0.0,
        mirror: false,
    },
];

const ENDERMAN_CUBES: &[EntityCube] = &[
    EntityCube {
        offset: [0.0, -13.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 0.0],
        cube: [-4.0, -8.0, -4.0, 8.0, 8.0, 8.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, -14.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [32.0, 16.0],
        cube: [-4.0, 0.0, -2.0, 8.0, 12.0, 4.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [-5.0, -12.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [56.0, 0.0],
        cube: [-1.0, -2.0, -1.0, 2.0, 30.0, 2.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [5.0, -12.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [56.0, 0.0],
        cube: [-1.0, -2.0, -1.0, 2.0, 30.0, 2.0],
        growth: 0.0,
        mirror: true,
    },
    EntityCube {
        offset: [-2.0, -5.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [56.0, 0.0],
        cube: [-1.0, 0.0, -1.0, 2.0, 30.0, 2.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [2.0, -5.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [56.0, 0.0],
        cube: [-1.0, 0.0, -1.0, 2.0, 30.0, 2.0],
        growth: 0.0,
        mirror: true,
    },
];

const SQUID_CUBES: &[EntityCube] = &[
    EntityCube {
        offset: [0.0, 8.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [0.0, 0.0],
        cube: [-6.0, -8.0, -6.0, 12.0, 16.0, 12.0],
        growth: 0.02,
        mirror: false,
    },
    EntityCube {
        offset: [5.0, 15.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [48.0, 0.0],
        cube: [-1.0, 0.0, -1.0, 2.0, 18.0, 2.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [3.5355, 15.0, 3.5355],
        rotation_x: 0.0,
        texture_origin: [48.0, 0.0],
        cube: [-1.0, 0.0, -1.0, 2.0, 18.0, 2.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 15.0, 5.0],
        rotation_x: 0.0,
        texture_origin: [48.0, 0.0],
        cube: [-1.0, 0.0, -1.0, 2.0, 18.0, 2.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [-3.5355, 15.0, 3.5355],
        rotation_x: 0.0,
        texture_origin: [48.0, 0.0],
        cube: [-1.0, 0.0, -1.0, 2.0, 18.0, 2.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [-5.0, 15.0, 0.0],
        rotation_x: 0.0,
        texture_origin: [48.0, 0.0],
        cube: [-1.0, 0.0, -1.0, 2.0, 18.0, 2.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [-3.5355, 15.0, -3.5355],
        rotation_x: 0.0,
        texture_origin: [48.0, 0.0],
        cube: [-1.0, 0.0, -1.0, 2.0, 18.0, 2.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [0.0, 15.0, -5.0],
        rotation_x: 0.0,
        texture_origin: [48.0, 0.0],
        cube: [-1.0, 0.0, -1.0, 2.0, 18.0, 2.0],
        growth: 0.0,
        mirror: false,
    },
    EntityCube {
        offset: [3.5355, 15.0, -3.5355],
        rotation_x: 0.0,
        texture_origin: [48.0, 0.0],
        cube: [-1.0, 0.0, -1.0, 2.0, 18.0, 2.0],
        growth: 0.0,
        mirror: false,
    },
];

const PIG_MODEL: EntityModel = EntityModel {
    texture_size: [64.0, 64.0],
    cubes: PIG_CUBES,
    rotated_cubes: &[],
};

const COW_MODEL: EntityModel = EntityModel {
    texture_size: [64.0, 64.0],
    cubes: COW_CUBES,
    rotated_cubes: &[],
};

const CHICKEN_MODEL: EntityModel = EntityModel {
    texture_size: [64.0, 32.0],
    cubes: CHICKEN_CUBES,
    rotated_cubes: &[],
};

const CREEPER_MODEL: EntityModel = EntityModel {
    texture_size: [64.0, 32.0],
    cubes: CREEPER_CUBES,
    rotated_cubes: &[],
};

const SHEEP_MODEL: EntityModel = EntityModel {
    texture_size: [64.0, 32.0],
    cubes: SHEEP_CUBES,
    rotated_cubes: &[],
};

const SHEEP_WOOL_MODEL: EntityModel = EntityModel {
    texture_size: [64.0, 32.0],
    cubes: SHEEP_WOOL_CUBES,
    rotated_cubes: &[],
};

const ZOMBIE_MODEL: EntityModel = EntityModel {
    texture_size: [64.0, 64.0],
    cubes: ZOMBIE_CUBES,
    rotated_cubes: &[],
};

const SKELETON_MODEL: EntityModel = EntityModel {
    texture_size: [64.0, 32.0],
    cubes: SKELETON_CUBES,
    rotated_cubes: &[],
};

const WOLF_MODEL: EntityModel = EntityModel {
    texture_size: [64.0, 32.0],
    cubes: WOLF_CUBES,
    rotated_cubes: &[],
};

const PARROT_MODEL: EntityModel = EntityModel {
    texture_size: [32.0, 32.0],
    cubes: PARROT_CUBES,
    rotated_cubes: &[],
};

const BAT_MODEL: EntityModel = EntityModel {
    texture_size: [32.0, 32.0],
    cubes: BAT_CUBES,
    rotated_cubes: &[],
};

const BEE_MODEL: EntityModel = EntityModel {
    texture_size: [64.0, 64.0],
    cubes: BEE_CUBES,
    rotated_cubes: &[],
};

const ENDERMAN_MODEL: EntityModel = EntityModel {
    texture_size: [64.0, 32.0],
    cubes: ENDERMAN_CUBES,
    rotated_cubes: &[],
};

const SQUID_MODEL: EntityModel = EntityModel {
    texture_size: [64.0, 32.0],
    cubes: SQUID_CUBES,
    rotated_cubes: &[],
};

fn sheep_wool_tint(color: u8) -> [f32; 3] {
    const TINTS: [[f32; 3]; 16] = [
        [0.901961, 0.901961, 0.901961],
        [0.729412, 0.376471, 0.082353],
        [0.584314, 0.227451, 0.552941],
        [0.168627, 0.525490, 0.639216],
        [0.745098, 0.635294, 0.176471],
        [0.376471, 0.584314, 0.090196],
        [0.713725, 0.407843, 0.498039],
        [0.207843, 0.231373, 0.239216],
        [0.458824, 0.458824, 0.443137],
        [0.062745, 0.458824, 0.458824],
        [0.400000, 0.145098, 0.541176],
        [0.176471, 0.200000, 0.498039],
        [0.384314, 0.247059, 0.145098],
        [0.274510, 0.364706, 0.062745],
        [0.517647, 0.133333, 0.109804],
        [0.082353, 0.082353, 0.094118],
    ];
    TINTS[(color & 15) as usize]
}

fn entity_model_position(model: [f32; 3], position: [f64; 3], yaw: f32) -> [f32; 3] {
    let x = -model[0] / 16.0;
    let y = 1.501 - model[1] / 16.0;
    let z = model[2] / 16.0;
    let (sin, cos) = (180.0 - yaw).to_radians().sin_cos();
    [
        position[0] as f32 + x * cos + z * sin,
        position[1] as f32 + y,
        position[2] as f32 - x * sin + z * cos,
    ]
}

fn entity_part_vertex(vertex: [f32; 3], offset: [f32; 3], rotation: [f32; 3]) -> [f32; 3] {
    let (sin_x, cos_x) = rotation[0].sin_cos();
    let (sin_y, cos_y) = rotation[1].sin_cos();
    let (sin_z, cos_z) = rotation[2].sin_cos();
    let y = vertex[1] * cos_x - vertex[2] * sin_x;
    let z = vertex[1] * sin_x + vertex[2] * cos_x;
    let x = vertex[0] * cos_y + z * sin_y;
    let z = -vertex[0] * sin_y + z * cos_y;
    let x_rot = x * cos_z - y * sin_z;
    let y_rot = x * sin_z + y * cos_z;
    [offset[0] + x_rot, offset[1] + y_rot, offset[2] + z]
}

#[allow(clippy::too_many_arguments)]
fn emit_entity_cube(
    atlas: &Atlas,
    mesh: &mut Mesh,
    position: [f64; 3],
    block_pos: [i32; 3],
    region: AtlasRegion,
    pass: TerrainPass,
    light: [f32; 2],
    texture_size: [f32; 2],
    tint: [f32; 3],
    offset: [f32; 3],
    rotation: [f32; 3],
    texture_origin: [f32; 2],
    cube: [f32; 6],
    growth: f32,
    mirror: bool,
    yaw: f32,
) {
    for (_, vertices, uvs) in model_box_faces(cube, texture_origin, mirror, growth) {
        let base = mesh.vertices.len() as u32;
        for (index, vertex) in vertices.iter().enumerate() {
            let model = entity_part_vertex(*vertex, offset, rotation);
            mesh.vertices.push(Vertex {
                position: entity_model_position(model, position, yaw),
                uv: atlas_uv_dimensions(region, atlas, &uvs[index], texture_size),
                color: tint,
                light,
            });
        }
        mesh.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        mesh.opaque_meta.push(QuadMeta {
            facing: FACING_UNASSIGNED,
            pass,
            block_pos,
            state: Block::AIR.default_state(),
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_entity_layer(
    data: &ResourcePackData,
    atlas: &Atlas,
    mesh: &mut Mesh,
    position: [f64; 3],
    block_pos: [i32; 3],
    light: [f32; 2],
    yaw: f32,
    texture_id: u32,
    texture_prefix: &str,
    model: &EntityModel,
    tint: [f32; 3],
) {
    let Some(path) = data.texture_table.get(texture_id as usize) else {
        return;
    };
    if !path.starts_with(texture_prefix) {
        return;
    }
    let pass = if atlas.region_has_alpha(texture_id) {
        TerrainPass::Cutout
    } else {
        TerrainPass::Solid
    };
    let region = atlas.region(texture_id);
    for cube in model.cubes {
        emit_entity_cube(
            atlas,
            mesh,
            position,
            block_pos,
            region,
            pass,
            light,
            model.texture_size,
            tint,
            cube.offset,
            [cube.rotation_x, 0.0, 0.0],
            cube.texture_origin,
            cube.cube,
            cube.growth,
            cube.mirror,
            yaw,
        );
    }
    for cube in model.rotated_cubes {
        emit_entity_cube(
            atlas,
            mesh,
            position,
            block_pos,
            region,
            pass,
            light,
            model.texture_size,
            tint,
            cube.offset,
            cube.rotation,
            cube.texture_origin,
            cube.cube,
            cube.growth,
            cube.mirror,
            yaw,
        );
    }
}

fn entity_variant(root: &Tag) -> &str {
    root.get("variant")
        .and_then(Tag::as_str)
        .unwrap_or("minecraft:temperate")
}

fn mesh_entities<A: BlockAccess + ?Sized>(
    data: &ResourcePackData,
    atlas: &Atlas,
    access: &A,
    mesh: &mut Mesh,
    chunk: &Chunk,
) {
    for entity in chunk.entities() {
        let Ok((_, root)) = mcre_anvil::nbt::read_named(entity.nbt()) else {
            continue;
        };
        if root
            .get("Age")
            .and_then(Tag::as_i32)
            .is_some_and(|age| age < 0)
        {
            continue;
        }
        let position = entity.position();
        let block_pos = [
            position[0].floor() as i32,
            position[1].floor() as i32,
            position[2].floor() as i32,
        ];
        let (block_light, sky_light) = sample_light(access, block_pos);
        let light = [f32::from(block_light), f32::from(sky_light)];
        let yaw = root
            .get("Rotation")
            .and_then(Tag::as_list)
            .and_then(|rotation| rotation.first())
            .and_then(Tag::as_f64)
            .unwrap_or(0.0) as f32;
        let white = [1.0, 1.0, 1.0];
        match entity.id() {
            "minecraft:pig" => {
                let variant = entity_variant(&root);
                if variant == "minecraft:cold" {
                    continue;
                }
                let texture_id = if variant == "minecraft:warm" {
                    data.entity_textures.pig_warm
                } else {
                    data.entity_textures.pig_temperate
                };
                emit_entity_layer(
                    data,
                    atlas,
                    mesh,
                    position,
                    block_pos,
                    light,
                    yaw,
                    texture_id,
                    "minecraft:entity/pig/",
                    &PIG_MODEL,
                    white,
                );
            }
            "minecraft:cow" => {
                if entity_variant(&root) != "minecraft:temperate" {
                    continue;
                }
                emit_entity_layer(
                    data,
                    atlas,
                    mesh,
                    position,
                    block_pos,
                    light,
                    yaw,
                    data.entity_textures.cow_temperate,
                    "minecraft:entity/cow/",
                    &COW_MODEL,
                    white,
                );
            }
            "minecraft:chicken" => {
                let variant = entity_variant(&root);
                if variant == "minecraft:cold" {
                    continue;
                }
                let texture_id = if variant == "minecraft:warm" {
                    data.entity_textures.chicken_warm
                } else {
                    data.entity_textures.chicken_temperate
                };
                emit_entity_layer(
                    data,
                    atlas,
                    mesh,
                    position,
                    block_pos,
                    light,
                    yaw,
                    texture_id,
                    "minecraft:entity/chicken/",
                    &CHICKEN_MODEL,
                    white,
                );
            }
            "minecraft:creeper" => {
                emit_entity_layer(
                    data,
                    atlas,
                    mesh,
                    position,
                    block_pos,
                    light,
                    yaw,
                    data.entity_textures.creeper,
                    "minecraft:entity/creeper/",
                    &CREEPER_MODEL,
                    white,
                );
            }
            "minecraft:zombie" => {
                emit_entity_layer(
                    data,
                    atlas,
                    mesh,
                    position,
                    block_pos,
                    light,
                    yaw,
                    data.entity_textures.zombie,
                    "minecraft:entity/zombie/",
                    &ZOMBIE_MODEL,
                    white,
                );
            }
            "minecraft:drowned" => {
                emit_entity_layer(
                    data,
                    atlas,
                    mesh,
                    position,
                    block_pos,
                    light,
                    yaw,
                    data.entity_textures.drowned,
                    "minecraft:entity/zombie/",
                    &ZOMBIE_MODEL,
                    white,
                );
            }
            "minecraft:husk" => {
                emit_entity_layer(
                    data,
                    atlas,
                    mesh,
                    position,
                    block_pos,
                    light,
                    yaw,
                    data.entity_textures.husk,
                    "minecraft:entity/zombie/",
                    &ZOMBIE_MODEL,
                    white,
                );
            }
            "minecraft:skeleton" => {
                emit_entity_layer(
                    data,
                    atlas,
                    mesh,
                    position,
                    block_pos,
                    light,
                    yaw,
                    data.entity_textures.skeleton,
                    "minecraft:entity/skeleton/",
                    &SKELETON_MODEL,
                    white,
                );
            }
            "minecraft:wolf" => {
                let texture_id = match entity_variant(&root) {
                    "minecraft:spotted" => data.entity_textures.wolf_spotted,
                    "minecraft:snowy" => data.entity_textures.wolf_snowy,
                    "minecraft:black" => data.entity_textures.wolf_black,
                    "minecraft:ashen" => data.entity_textures.wolf_ashen,
                    "minecraft:rusty" => data.entity_textures.wolf_rusty,
                    "minecraft:woods" => data.entity_textures.wolf_woods,
                    "minecraft:chestnut" => data.entity_textures.wolf_chestnut,
                    "minecraft:striped" => data.entity_textures.wolf_striped,
                    _ => data.entity_textures.wolf_pale,
                };
                emit_entity_layer(
                    data,
                    atlas,
                    mesh,
                    position,
                    block_pos,
                    light,
                    yaw,
                    texture_id,
                    "minecraft:entity/wolf/",
                    &WOLF_MODEL,
                    white,
                );
            }
            "minecraft:parrot" => {
                let texture_id = match root.get("Variant").and_then(Tag::as_i32).unwrap_or(0) {
                    1 => data.entity_textures.parrot_blue,
                    2 => data.entity_textures.parrot_green,
                    3 => data.entity_textures.parrot_yellow_blue,
                    4 => data.entity_textures.parrot_grey,
                    _ => data.entity_textures.parrot_red_blue,
                };
                emit_entity_layer(
                    data,
                    atlas,
                    mesh,
                    position,
                    block_pos,
                    light,
                    yaw,
                    texture_id,
                    "minecraft:entity/parrot/",
                    &PARROT_MODEL,
                    white,
                );
            }
            "minecraft:bat" => {
                emit_entity_layer(
                    data,
                    atlas,
                    mesh,
                    position,
                    block_pos,
                    light,
                    yaw,
                    data.entity_textures.bat,
                    "minecraft:entity/bat/",
                    &BAT_MODEL,
                    white,
                );
            }
            "minecraft:bee" => {
                emit_entity_layer(
                    data,
                    atlas,
                    mesh,
                    position,
                    block_pos,
                    light,
                    yaw,
                    data.entity_textures.bee,
                    "minecraft:entity/bee/",
                    &BEE_MODEL,
                    white,
                );
            }
            "minecraft:enderman" => {
                emit_entity_layer(
                    data,
                    atlas,
                    mesh,
                    position,
                    block_pos,
                    light,
                    yaw,
                    data.entity_textures.enderman,
                    "minecraft:entity/enderman/",
                    &ENDERMAN_MODEL,
                    white,
                );
            }
            "minecraft:squid" => {
                emit_entity_layer(
                    data,
                    atlas,
                    mesh,
                    position,
                    block_pos,
                    light,
                    yaw,
                    data.entity_textures.squid,
                    "minecraft:entity/squid/",
                    &SQUID_MODEL,
                    white,
                );
            }
            "minecraft:glow_squid" => {
                emit_entity_layer(
                    data,
                    atlas,
                    mesh,
                    position,
                    block_pos,
                    light,
                    yaw,
                    data.entity_textures.glow_squid,
                    "minecraft:entity/squid/",
                    &SQUID_MODEL,
                    white,
                );
            }
            "minecraft:sheep" => {
                emit_entity_layer(
                    data,
                    atlas,
                    mesh,
                    position,
                    block_pos,
                    light,
                    yaw,
                    data.entity_textures.sheep,
                    "minecraft:entity/sheep/",
                    &SHEEP_MODEL,
                    white,
                );
                let sheared = matches!(root.get("Sheared"), Some(Tag::Byte(value)) if *value != 0);
                if !sheared {
                    let color = match root.get("Color") {
                        Some(Tag::Byte(value)) => (*value as u8) & 15,
                        _ => 0,
                    };
                    emit_entity_layer(
                        data,
                        atlas,
                        mesh,
                        position,
                        block_pos,
                        light,
                        yaw,
                        data.entity_textures.sheep_wool,
                        "minecraft:entity/sheep/",
                        &SHEEP_WOOL_MODEL,
                        sheep_wool_tint(color),
                    );
                }
            }
            _ => {}
        }
    }
}

fn sign_text_color(name: &str, glowing: bool) -> [f32; 3] {
    let rgb = match name {
        "white" => [255, 255, 255],
        "orange" => [255, 104, 31],
        "magenta" => [255, 0, 255],
        "light_blue" => [154, 192, 205],
        "yellow" => [255, 255, 0],
        "lime" => [191, 255, 0],
        "pink" => [255, 105, 180],
        "gray" => [128, 128, 128],
        "light_gray" => [211, 211, 211],
        "cyan" => [0, 255, 255],
        "purple" => [160, 32, 240],
        "blue" => [0, 0, 255],
        "brown" => [139, 69, 19],
        "green" => [0, 255, 0],
        "red" => [255, 0, 0],
        _ => [0, 0, 0],
    };
    let scale = if glowing { 1.0 } else { 0.4 };
    rgb.map(|component| ((component as f32 * scale) as u8) as f32 / 255.0)
}

/// Vanilla `Block.getShadeBrightness`: collision-full cells darken
/// neighboring corner samples to 0.2, with per-block overrides (glass and
/// friends stay at 1.0, snow layer 8 and soul sand reach 0.2). Extracted
/// from the game at data-gen time.
fn shade_of(state: BlockState) -> f32 {
    state.shade_brightness()
}

/// Vanilla `BlockStateBase.isLightPermeable` (BlockBehaviour.java:812).
fn is_light_permeable(state: BlockState) -> bool {
    !state.solid_render() || state.light_dampening() == 0
}

/// Vanilla `LightCoordsUtil.getLightCoords`: stored cell light raised to the
/// sampling block's own emission.
pub(crate) fn sample_light<A: BlockAccess + ?Sized>(access: &A, pos: [i32; 3]) -> (u8, u8) {
    let state = access.block_at(pos[0], pos[1], pos[2]);
    if state.emissive_rendering() {
        return (15, 15);
    }
    let (block, sky) = access.light_at(pos[0], pos[1], pos[2]);
    (block.max(state.light_emission()), sky)
}

fn offset(pos: [i32; 3], direction: Direction) -> [i32; 3] {
    let delta = direction.delta();
    [pos[0] + delta[0], pos[1] + delta[1], pos[2] + delta[2]]
}

/// The four edge cells sampled around a face, relative to the neighbor cell
/// across that face (`BlockModelLighter.AdjacencyInfo.corners`).
fn adjacency_corners(direction: Direction) -> [Direction; 4] {
    match direction {
        Direction::Down => [
            Direction::West,
            Direction::East,
            Direction::North,
            Direction::South,
        ],
        Direction::Up => [
            Direction::East,
            Direction::West,
            Direction::North,
            Direction::South,
        ],
        Direction::North => [
            Direction::Up,
            Direction::Down,
            Direction::East,
            Direction::West,
        ],
        Direction::South => [
            Direction::West,
            Direction::East,
            Direction::Down,
            Direction::Up,
        ],
        Direction::West => [
            Direction::Up,
            Direction::Down,
            Direction::North,
            Direction::South,
        ],
        Direction::East => [
            Direction::Down,
            Direction::Up,
            Direction::North,
            Direction::South,
        ],
    }
}

/// Maps computed vertex pairings to baked quad vertex slots
/// (`BlockModelLighter.AmbientVertexRemap`).
fn vertex_remap(direction: Direction) -> [usize; 4] {
    match direction {
        Direction::Down => [0, 1, 2, 3],
        Direction::Up => [2, 3, 0, 1],
        Direction::North => [3, 0, 1, 2],
        Direction::South => [0, 1, 2, 3],
        Direction::West => [3, 0, 1, 2],
        Direction::East => [1, 2, 3, 0],
    }
}

/// Port of vanilla `LightCoordsUtil.smoothBlend`, per channel: fully dark
/// neighbors inherit the center level (partially dark ones inherit only the
/// sky channel) before the four samples are averaged.
fn smooth_blend(n1: (u8, u8), n2: (u8, u8), n3: (u8, u8), center: (u8, u8)) -> (f32, f32) {
    let mut neighbors = [n1, n2, n3];
    if center.0 > 2 || center.1 > 2 {
        for neighbor in &mut neighbors {
            if neighbor.0 == 0 && neighbor.1 == 0 {
                *neighbor = center;
            } else if neighbor.1 == 0 {
                neighbor.1 = center.1;
            }
        }
    }
    let block = f32::from(neighbors[0].0 + neighbors[1].0 + neighbors[2].0 + center.0) * 0.25;
    let sky = f32::from(neighbors[0].1 + neighbors[1].1 + neighbors[2].1 + center.1) * 0.25;
    (block, sky)
}

/// Vanilla `prepareQuadFlat`: one light sample, raised to the quad owner's
/// emission; no corner AO. Culled and cubic faces sample the neighbor cell,
/// partial unculled faces sample the owning cell.
fn flat_face<A: BlockAccess + ?Sized>(
    access: &A,
    direction: Direction,
    block_pos: [i32; 3],
    verts: &[mcre_core::Vec3f; 4],
    culled: bool,
    owner: BlockState,
    color: [f32; 3],
) -> ([[f32; 3]; 4], [[f32; 2]; 4]) {
    let bounds = face_bounds(verts);
    let cubic = face_cubic(direction, &bounds, owner.collision_full());
    let sample = if culled || cubic {
        offset(block_pos, direction)
    } else {
        block_pos
    };
    let (block, sky) = access.light_at(sample[0], sample[1], sample[2]);
    let (block, sky) = if owner.emissive_rendering() {
        (15, 15)
    } else {
        (block.max(owner.light_emission()), sky)
    };
    let light = [f32::from(block), f32::from(sky)];
    ([color; 4], [light; 4])
}

/// Vanilla `prepareQuadAmbientOcclusion`: per-vertex blend of two edge cells,
/// their diagonal, and the face-center cell, with the four shade samples
/// averaged into an AO multiplier. Cubic faces sample around the cell in
/// front; partial faces sample around the owning cell itself.
///
/// Deviation from vanilla: the edge-cell pair for each vertex is derived from
/// the vertex position relative to the block center instead of the vertex
/// slot (`AmbientVertexRemap`). Slot attachment permutes with model-variant
/// rotation, dragging AO corners along with the texture; position attachment
/// keeps AO fixed in world space so only the texture rotates. For authored
/// vertex orders both agree exactly.
fn smooth_face<A: BlockAccess + ?Sized>(
    access: &A,
    direction: Direction,
    block_pos: [i32; 3],
    verts: &[mcre_core::Vec3f; 4],
    color: [f32; 3],
    owner: BlockState,
) -> ([[f32; 3]; 4], [[f32; 2]; 4]) {
    let bounds = face_bounds(verts);
    let partial = face_partial(direction, &bounds);
    let cubic = face_cubic(direction, &bounds, owner.collision_full());
    let front = offset(block_pos, direction);
    // Cubic faces sample around the cell in front; partial faces sample
    // around the owning cell itself (vanilla `basePosition`).
    let base = if cubic { front } else { block_pos };
    let corners = adjacency_corners(direction);

    let mut edge_light = [(0_u8, 0_u8); 4];
    let mut edge_shade = [1.0_f32; 4];
    let mut edge_permeable = [false; 4];
    for (index, &corner) in corners.iter().enumerate() {
        let pos = offset(base, corner);
        edge_light[index] = sample_light(access, pos);
        let state = access.block_at(pos[0], pos[1], pos[2]);
        edge_shade[index] = shade_of(state);
        // The diagonal-reuse gate reads the cell one step further along the
        // face normal (vanilla samples `basePosition + corner + direction`).
        let gate = offset(pos, direction);
        edge_permeable[index] = is_light_permeable(access.block_at(gate[0], gate[1], gate[2]));
    }

    // Diagonal cells: reused from the first edge sample when both adjacent
    // cells block light, otherwise sampled directly (vanilla exactly).
    let diagonal = |a: usize, b: usize| -> ((u8, u8), f32) {
        if !edge_permeable[b] && !edge_permeable[a] {
            return (edge_light[0], edge_shade[0]);
        }
        let pos = offset(offset(base, corners[a]), corners[b]);
        let state = access.block_at(pos[0], pos[1], pos[2]);
        (sample_light(access, pos), shade_of(state))
    };
    let diagonals = [
        diagonal(0, 3),
        diagonal(0, 2),
        diagonal(1, 2),
        diagonal(1, 3),
    ];

    let neighbor = access.block_at(front[0], front[1], front[2]);
    let center_light = if cubic || !neighbor.solid_render() {
        sample_light(access, front)
    } else {
        sample_light(access, block_pos)
    };
    let center_shade = if cubic {
        shade_of(neighbor)
    } else {
        shade_of(owner)
    };

    let pairings = [(3, 0), (2, 0), (2, 1), (3, 1)];
    let mut tblend = [(0.0_f32, 0.0_f32); 4];
    let mut tshade = [0.0_f32; 4];
    for (k, &(a, b)) in pairings.iter().enumerate() {
        tblend[k] = smooth_blend(edge_light[a], edge_light[b], diagonals[k].0, center_light);
        tshade[k] = (edge_shade[a] + edge_shade[b] + diagonals[k].1 + center_shade) * 0.25;
    }

    let mut lights = [[0.0_f32; 2]; 4];
    let mut colors = [[1.0_f32; 3]; 4];
    if !partial {
        let remap = vertex_remap(direction);
        let mut inv_remap = [0; 4];
        for (computed, &slot) in remap.iter().enumerate() {
            inv_remap[slot] = computed;
        }
        for (i, vert) in verts.iter().enumerate() {
            let (a, b) = corner_edges(direction, vert, &corners).unwrap_or_else(|| {
                let (a, b) = pairings[inv_remap[i]];
                (a, b)
            });
            let (diag_light, diag_shade) = diagonal(a, b);
            let blended = smooth_blend(edge_light[a], edge_light[b], diag_light, center_light);
            let ambient = (edge_shade[a] + edge_shade[b] + diag_shade + center_shade) * 0.25;
            lights[i] = [blended.0, blended.1];
            colors[i] = [color[0] * ambient, color[1] * ambient, color[2] * ambient];
        }
        return (colors, lights);
    }

    let rows = noncubic_weights(direction);
    let remap = vertex_remap(direction);
    let mut inv_remap = [0; 4];
    for (computed, &slot) in remap.iter().enumerate() {
        inv_remap[slot] = computed;
    }
    for (i, vert) in verts.iter().enumerate() {
        let row = match corner_edges(direction, vert, &corners) {
            Some((a, b)) => rows[weight_row(direction, a, b)],
            None => rows[inv_remap[i]],
        };
        let weights = [
            bounds[row[0] as usize] * bounds[row[1] as usize],
            bounds[row[2] as usize] * bounds[row[3] as usize],
            bounds[row[4] as usize] * bounds[row[5] as usize],
            bounds[row[6] as usize] * bounds[row[7] as usize],
        ];
        let blended = smooth_weighted_blend(tblend, weights);
        let ambient = (tshade[0] * weights[0]
            + tshade[1] * weights[1]
            + tshade[2] * weights[2]
            + tshade[3] * weights[3])
            .clamp(0.0, 1.0);
        lights[i] = [blended.0, blended.1];
        colors[i] = [color[0] * ambient, color[1] * ambient, color[2] * ambient];
    }
    (colors, lights)
}

fn smooth_weighted_blend(lights: [(f32, f32); 4], weights: [f32; 4]) -> (f32, f32) {
    let block = lights
        .iter()
        .zip(weights)
        .map(|(l, w)| l.0 * w)
        .sum::<f32>();
    let sky = lights
        .iter()
        .zip(weights)
        .map(|(l, w)| l.1 * w)
        .sum::<f32>();
    let quantize = |value: f32| (value.clamp(0.0, 15.0) * 16.0).floor() / 16.0;
    (quantize(block), quantize(sky))
}

/// Normalized face bounds in SizeInfo order: min/max per axis then flips.
fn face_bounds(verts: &[mcre_core::Vec3f; 4]) -> [f32; 12] {
    let (mut min_x, mut min_y, mut min_z) = (f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let (mut max_x, mut max_y, mut max_z) =
        (f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for v in verts {
        let (x, y, z) = (v[0] / 16.0, v[1] / 16.0, v[2] / 16.0);
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        min_z = min_z.min(z);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
        max_z = max_z.max(z);
    }
    [
        min_y,
        max_y,
        min_z,
        max_z,
        min_x,
        max_x,
        1.0 - min_y,
        1.0 - max_y,
        1.0 - min_z,
        1.0 - max_z,
        1.0 - min_x,
        1.0 - max_x,
    ]
}

fn face_partial(direction: Direction, bounds: &[f32; 12]) -> bool {
    match direction {
        Direction::Down | Direction::Up => {
            bounds[4] >= 1e-4 || bounds[2] >= 1e-4 || bounds[5] <= 0.9999 || bounds[3] <= 0.9999
        }
        Direction::North | Direction::South => {
            bounds[4] >= 1e-4 || bounds[0] >= 1e-4 || bounds[5] <= 0.9999 || bounds[1] <= 0.9999
        }
        Direction::West | Direction::East => {
            bounds[0] >= 1e-4 || bounds[2] >= 1e-4 || bounds[1] <= 0.9999 || bounds[3] <= 0.9999
        }
    }
}

fn face_cubic(direction: Direction, bounds: &[f32; 12], collision_full: bool) -> bool {
    let planar = match direction {
        Direction::Down | Direction::Up => (bounds[0] - bounds[1]).abs() < f32::EPSILON,
        Direction::North | Direction::South => (bounds[2] - bounds[3]).abs() < f32::EPSILON,
        Direction::West | Direction::East => (bounds[4] - bounds[5]).abs() < f32::EPSILON,
    };
    if !planar {
        return false;
    }
    let at_boundary = match direction {
        Direction::Down => bounds[0] < 1e-4,
        Direction::Up => bounds[1] > 0.9999,
        Direction::North => bounds[2] < 1e-4,
        Direction::South => bounds[3] > 0.9999,
        Direction::West => bounds[4] < 1e-4,
        Direction::East => bounds[5] > 0.9999,
    };
    at_boundary || collision_full
}

/// Weight-table row for a world-space corner, keeping AO fixed under
/// model-variant rotation (vanilla indexes rows by slot, which permutes).
/// Pairs are edge indices in `adjacency_corners` order as produced by
/// `corner_edges`.
fn weight_row(direction: Direction, a: usize, b: usize) -> usize {
    match (direction, a, b) {
        (Direction::Down, 0, 3) => 0,
        (Direction::Down, 0, 2) => 1,
        (Direction::Down, 1, 2) => 2,
        (Direction::Down, 1, 3) => 3,
        (Direction::Up, 1, 2) => 2,
        (Direction::Up, 1, 3) => 3,
        (Direction::Up, 0, 3) => 0,
        (Direction::Up, 0, 2) => 1,
        (Direction::North, 2, 0) => 1,
        (Direction::North, 2, 1) => 2,
        (Direction::North, 3, 1) => 3,
        (Direction::North, 3, 0) => 0,
        (Direction::South, 0, 3) => 0,
        (Direction::South, 0, 2) => 1,
        (Direction::South, 1, 2) => 2,
        (Direction::South, 1, 3) => 3,
        (Direction::West, 2, 0) => 1,
        (Direction::West, 2, 1) => 2,
        (Direction::West, 3, 1) => 3,
        (Direction::West, 3, 0) => 0,
        (Direction::East, 3, 1) => 3,
        (Direction::East, 3, 0) => 0,
        (Direction::East, 2, 0) => 1,
        (Direction::East, 2, 1) => 2,
        _ => 0,
    }
}

fn noncubic_weights(direction: Direction) -> [[u8; 8]; 4] {
    match direction {
        Direction::Down => [
            [10, 3, 10, 9, 4, 9, 4, 3],
            [10, 2, 10, 8, 4, 8, 4, 2],
            [11, 2, 11, 8, 5, 8, 5, 2],
            [11, 3, 11, 9, 5, 9, 5, 3],
        ],
        Direction::Up => [
            [5, 3, 5, 9, 11, 9, 11, 3],
            [5, 2, 5, 8, 11, 8, 11, 2],
            [4, 2, 4, 8, 10, 8, 10, 2],
            [4, 3, 4, 9, 10, 9, 10, 3],
        ],
        Direction::North => [
            [1, 10, 1, 4, 7, 4, 7, 10],
            [1, 11, 1, 5, 7, 5, 7, 11],
            [0, 11, 0, 5, 6, 5, 6, 11],
            [0, 10, 0, 4, 6, 4, 6, 10],
        ],
        Direction::South => [
            [1, 10, 7, 10, 7, 4, 1, 4],
            [0, 10, 6, 10, 6, 4, 0, 4],
            [0, 11, 6, 11, 6, 5, 0, 5],
            [1, 11, 7, 11, 7, 5, 1, 5],
        ],
        Direction::West => [
            [1, 3, 1, 9, 7, 9, 7, 3],
            [1, 2, 1, 8, 7, 8, 7, 2],
            [0, 2, 0, 8, 6, 8, 6, 2],
            [0, 3, 0, 9, 6, 9, 6, 3],
        ],
        Direction::East => [
            [6, 3, 6, 9, 0, 9, 0, 3],
            [6, 2, 6, 8, 0, 8, 0, 2],
            [7, 2, 7, 8, 1, 8, 1, 2],
            [7, 3, 7, 9, 1, 9, 1, 3],
        ],
    }
}

/// Edge-cell pair for a quad vertex from its position relative to the block
/// center, so AO corners stay fixed in world space under model-variant
/// rotation. Returns `None` when the vertex sits exactly on the face center
/// plane along either tangent axis.
fn corner_edges(
    direction: Direction,
    vert: &mcre_core::Vec3f,
    corners: &[Direction; 4],
) -> Option<(usize, usize)> {
    let (ax0, ax1) = match direction {
        Direction::Down | Direction::Up => (0, 2),
        Direction::North | Direction::South => (0, 1),
        Direction::West | Direction::East => (2, 1),
    };
    let axis_dir = |axis: usize, rel: f32| -> Option<Direction> {
        let positive = if rel > 0.0 {
            true
        } else if rel < 0.0 {
            false
        } else {
            return None;
        };
        Some(match (axis, positive) {
            (0, false) => Direction::West,
            (0, true) => Direction::East,
            (1, false) => Direction::Down,
            (1, true) => Direction::Up,
            (_, false) => Direction::North,
            (_, true) => Direction::South,
        })
    };
    let rel = [vert[0] - 8.0, vert[1] - 8.0, vert[2] - 8.0];
    let a = axis_dir(ax0, rel[ax0])?;
    let b = axis_dir(ax1, rel[ax1])?;
    let ai = corners.iter().position(|&d| d == a)?;
    let bi = corners.iter().position(|&d| d == b)?;
    Some((ai, bi))
}

/// Vanilla `Mth.getSeed`: per-block-position model seed (Java int/long
/// wrapping arithmetic).
fn position_seed(x: i32, y: i32, z: i32) -> i64 {
    let mut seed =
        (x.wrapping_mul(3129871) as i64) ^ (z as i64).wrapping_mul(116129781) ^ (y as i64);
    seed = seed
        .wrapping_mul(seed)
        .wrapping_mul(42317861)
        .wrapping_add(seed.wrapping_mul(11));
    seed >> 16
}

/// Port of vanilla `SingleThreadedRandomSource` (`java.util.Random` LCG).
struct LegacyRandom {
    seed: i64,
}

impl LegacyRandom {
    const MULTIPLIER: i64 = 25214903917;
    const MASK: i64 = (1 << 48) - 1;

    fn new(seed: i64) -> Self {
        let mut rng = Self { seed: 0 };
        rng.set_seed(seed);
        rng
    }

    fn set_seed(&mut self, seed: i64) {
        self.seed = (seed ^ Self::MULTIPLIER) & Self::MASK;
    }

    fn next(&mut self, bits: u32) -> i32 {
        self.seed = (self.seed.wrapping_mul(Self::MULTIPLIER).wrapping_add(11)) & Self::MASK;
        (self.seed >> (48 - bits)) as i32
    }

    fn next_int(&mut self, bound: i32) -> i32 {
        assert!(bound > 0);
        if (bound & (bound - 1)) == 0 {
            ((bound as i64 * self.next(31) as i64) >> 31) as i32
        } else {
            loop {
                let sample = self.next(31);
                let modulo = sample % bound;
                if sample.wrapping_sub(modulo).wrapping_add(bound - 1) >= 0 {
                    return modulo;
                }
            }
        }
    }

    fn next_long(&mut self) -> i64 {
        ((self.next(32) as i64) << 32) + self.next(32) as i64
    }
}

/// Vanilla `WeightedList` pick: first entry whose cumulative weight exceeds
/// the drawn selection.
fn pick_variant<'a>(rule: &'a BlockRule, rng: &mut LegacyRandom) -> &'a BlockVariant {
    let total: i32 = rule.variants.iter().map(|v| i32::from(v.weight)).sum();
    let mut selection = rng.next_int(total);
    for variant in &rule.variants {
        selection -= i32::from(variant.weight);
        if selection < 0 {
            return variant;
        }
    }
    rule.variants.last().expect("rule has no variants")
}

/// Vanilla `ModelBlockRenderer.tesselateBlock` model selection: a lone rule
/// emits its single variant untouched and draws only for weighted
/// multi-variant models; multipart rules share one `nextLong`, re-seeded per
/// part (`MultiPartModel.collectParts`).
fn for_each_variant(
    entry: &mcre_resource_pack::lookup::BlockStateEntry,
    pos: [i32; 3],
    mut emit: impl FnMut(&BlockVariant),
) {
    let mut rules = entry.rules.iter().filter(|rule| !rule.variants.is_empty());
    let Some(first) = rules.next() else { return };
    let Some(second) = rules.next() else {
        if first.variants.len() == 1 {
            emit(&first.variants[0]);
        } else {
            let mut rng = LegacyRandom::new(position_seed(pos[0], pos[1], pos[2]));
            emit(pick_variant(first, &mut rng));
        }
        return;
    };
    let mut rng = LegacyRandom::new(position_seed(pos[0], pos[1], pos[2]));
    let seed = rng.next_long();
    for rule in [first, second].into_iter().chain(rules) {
        rng.set_seed(seed);
        if rule.variants.len() == 1 {
            emit(&rule.variants[0]);
        } else {
            emit(pick_variant(rule, &mut rng));
        }
    }
}

/// Vanilla `Block.shouldRenderFace`, in order: a unit-cube occluder culls
/// outright; then the extracted `skipRendering` data; then an empty occluder
/// or an empty face shape of the state itself keeps the face; otherwise the
/// face renders when the state's footprint pokes outside the occluder's.
fn should_render_face(
    data: &ResourcePackData,
    state: BlockState,
    neighbor: BlockState,
    direction: Direction,
    options: MeshOptions,
) -> bool {
    if neighbor.solid_render() {
        return false;
    }
    if skip_rendering(data, state, neighbor, direction, options) {
        return false;
    }
    let neighbor_boxes = neighbor.occlusion_boxes();
    let occluder = face_boxes(neighbor_boxes, direction.opposite());
    if occluder.is_empty() {
        return true;
    }
    if face_shape_is_cube(neighbor_boxes, direction.opposite()) {
        return false;
    }
    let shape = face_boxes(state.occlusion_boxes(), direction);
    if shape.is_empty() {
        return true;
    }
    !is_covered(&shape, &occluder)
}

/// Data-driven vanilla `Block.skipRendering` (extracted per state/block in
/// `data_gen`): same-block self-skip bits, liquid-fluid match, fast-leaves
/// pair skip, bars connection rule from extracted tag membership and
/// connection props, and skip-against/culling tag overlap. Pack-level
/// `mcre/culling.json` overrides replace the generated tag lists per block.
fn skip_rendering(
    data: &ResourcePackData,
    state: BlockState,
    neighbor: BlockState,
    direction: Direction,
    options: MeshOptions,
) -> bool {
    if state.block() == neighbor.block() && state.self_skip() >> direction.index() & 1 == 1 {
        return true;
    }
    if let Some(fluid) = state.block().liquid_fluid()
        && neighbor.fluid() == Some(fluid)
    {
        return true;
    }
    if options.fast_leaves && in_leaves(state.block()) && in_leaves(neighbor.block()) {
        return true;
    }
    if in_bars(state.block()) && in_bars(neighbor.block()) {
        if direction.axis() == Axis::Y {
            return true;
        }
        return connection_prop(state, direction)
            && connection_prop(neighbor, direction.opposite());
    }

    let block = state.block();
    let neighbor_block = neighbor.block();
    let has_skip_tag = |tag: &str| -> bool {
        match data
            .culling
            .get(block.name())
            .and_then(|overrides| overrides.skip_against_tags.as_ref())
        {
            Some(list) => list.iter().any(|t| t == tag),
            None => block.skip_against_tags().contains(&tag),
        }
    };
    match data
        .culling
        .get(neighbor_block.name())
        .and_then(|overrides| overrides.culling_tags.as_ref())
    {
        Some(list) => list.iter().any(|t| has_skip_tag(t)),
        None => neighbor_block
            .culling_tags()
            .iter()
            .any(|t| has_skip_tag(t)),
    }
}

fn in_leaves(block: Block) -> bool {
    block.culling_tags().contains(&"leaves")
}

fn in_bars(block: Block) -> bool {
    block.culling_tags().contains(&"bars")
}

fn connection_prop(state: BlockState, direction: Direction) -> bool {
    match direction {
        Direction::North => state.is_north(),
        Direction::South => state.is_south(),
        Direction::East => state.is_east(),
        Direction::West => state.is_west(),
        Direction::Down | Direction::Up => false,
    }
}

/// Maps model-space UVs (0..16) to normalized atlas coordinates. Regions pack
/// edge-to-edge with zero gutter; anti-bleed is handled by the packed format's
/// centroid-biased texcoord encoding (Sodium `encodeTexture`), so no inset is
/// applied here.
pub(crate) fn atlas_uv(region: AtlasRegion, atlas: &Atlas, uv: &[f32; 2]) -> [f32; 2] {
    atlas_uv_dimensions(region, atlas, uv, [16.0, 16.0])
}

fn atlas_uv_dimensions(
    region: AtlasRegion,
    atlas: &Atlas,
    uv: &[f32; 2],
    texture_size: [f32; 2],
) -> [f32; 2] {
    let u = (uv[0] / texture_size[0]).clamp(0.0, 1.0);
    let v = (uv[1] / texture_size[1]).clamp(0.0, 1.0);
    let x0 = region.x as f32;
    let x1 = (region.x + region.width) as f32;
    let y0 = region.y as f32;
    let y1 = (region.y + region.height) as f32;
    let px = x0 + u * (x1 - x0);
    let py = y0 + v * (y1 - y0);
    [px / atlas.width as f32, py / atlas.height as f32]
}

/// Combines the baked vanilla face shade factor with the block tint color
/// (vanilla `BlockColors` layer selected by `tintindex`).
fn quad_color<A: BlockAccess + ?Sized>(
    data: &ResourcePackData,
    access: &A,
    state: BlockState,
    block_pos: [i32; 3],
    quad: &mcre_resource_pack::lookup::BakedQuadEntry,
) -> [f32; 3] {
    let tint = quad_tint(data, access, state, block_pos, quad);
    [
        quad.shade * tint[0],
        quad.shade * tint[1],
        quad.shade * tint[2],
    ]
}

fn rgb_to_float(rgb: [u8; 3]) -> [f32; 3] {
    [
        f32::from(rgb[0]) / 255.0,
        f32::from(rgb[1]) / 255.0,
        f32::from(rgb[2]) / 255.0,
    ]
}

/// Vanilla `BlockTintSources.water()` / `BiomeColors.getAverageWaterColor`.
pub(crate) fn water_tint<A: BlockAccess + ?Sized>(
    data: &ResourcePackData,
    access: &A,
    pos: [i32; 3],
) -> [f32; 3] {
    rgb_to_float(average_biome_color(
        access,
        pos,
        &data.colormaps,
        biome_water_color,
    ))
}

fn quad_tint<A: BlockAccess + ?Sized>(
    data: &ResourcePackData,
    access: &A,
    state: BlockState,
    block_pos: [i32; 3],
    quad: &mcre_resource_pack::lookup::BakedQuadEntry,
) -> [f32; 3] {
    let Some(tintindex) = quad.tintindex else {
        return [1.0; 3];
    };
    let Some(source) = tint_sources(state.block().name()).get(tintindex as usize) else {
        return [1.0; 3];
    };
    rgb_to_float(tint_source_color(data, access, state, block_pos, *source))
}

pub fn particle_tint<A: BlockAccess + ?Sized>(
    data: &ResourcePackData,
    access: &A,
    state: BlockState,
    block_pos: [i32; 3],
) -> [f32; 3] {
    let Some(source) = tint_sources(state.block().name()).first() else {
        return [1.0; 3];
    };
    rgb_to_float(tint_source_color(data, access, state, block_pos, *source))
}

fn tint_source_color<A: BlockAccess + ?Sized>(
    data: &ResourcePackData,
    access: &A,
    state: BlockState,
    block_pos: [i32; 3],
    source: TintSource,
) -> [u8; 3] {
    match source {
        TintSource::Blank => [255, 255, 255],
        TintSource::Constant(rgb) => rgb,
        TintSource::Grass | TintSource::SugarCane => {
            average_biome_color(access, block_pos, &data.colormaps, biome_grass_color)
        }
        TintSource::DoubleTallGrass => {
            let pos = if state.doubleblock_half() == DoubleBlockHalf::Upper {
                [block_pos[0], block_pos[1] - 1, block_pos[2]]
            } else {
                block_pos
            };
            average_biome_color(access, pos, &data.colormaps, biome_grass_color)
        }
        TintSource::Foliage => {
            average_biome_color(access, block_pos, &data.colormaps, biome_foliage_color)
        }
        TintSource::DryFoliage => {
            average_biome_color(access, block_pos, &data.colormaps, biome_dry_foliage_color)
        }
        TintSource::Water => {
            average_biome_color(access, block_pos, &data.colormaps, biome_water_color)
        }
        TintSource::Redstone => redstone_color(state.power()),
        TintSource::Stem => stem_color(state.age()),
    }
}

fn average_biome_color<A: BlockAccess + ?Sized>(
    access: &A,
    pos: [i32; 3],
    colormaps: &Colormaps,
    resolve: impl Fn(Biome, i32, i32, &Colormaps) -> [u8; 3],
) -> [u8; 3] {
    let mut red = 0u32;
    let mut green = 0u32;
    let mut blue = 0u32;
    for dx in -TINT_BLEND_RADIUS..=TINT_BLEND_RADIUS {
        for dz in -TINT_BLEND_RADIUS..=TINT_BLEND_RADIUS {
            let [r, g, b] = resolve(
                access.biome_at(pos[0] + dx, pos[1], pos[2] + dz),
                pos[0] + dx,
                pos[2] + dz,
                colormaps,
            );
            red += u32::from(r);
            green += u32::from(g);
            blue += u32::from(b);
        }
    }
    let count = ((TINT_BLEND_RADIUS * 2 + 1) * (TINT_BLEND_RADIUS * 2 + 1)) as u32;
    [
        (red / count) as u8,
        (green / count) as u8,
        (blue / count) as u8,
    ]
}

fn biome_grass_color(biome: Biome, x: i32, z: i32, colormaps: &Colormaps) -> [u8; 3] {
    if let Some(override_color) = biome.grass_color_override() {
        return override_color;
    }
    let base = sample_colormap(
        &colormaps.grass,
        biome.temperature(),
        biome.downfall(),
        GRASS_DEFAULT,
    );
    match biome.grass_color_modifier() {
        GrassColorModifier::None => base,
        GrassColorModifier::DarkForest => {
            let packed =
                (u32::from(base[0]) << 16) | (u32::from(base[1]) << 8) | u32::from(base[2]);
            let darkened = ((packed & 0xFEFEFE) + 0x28340A) >> 1;
            [
                (darkened >> 16) as u8,
                (darkened >> 8) as u8,
                darkened as u8,
            ]
        }
        GrassColorModifier::Swamp => mcre_core::noise::swamp_grass_tint(x, z),
    }
}

fn biome_foliage_color(biome: Biome, _x: i32, _z: i32, colormaps: &Colormaps) -> [u8; 3] {
    biome.foliage_color_override().unwrap_or_else(|| {
        sample_colormap(
            &colormaps.foliage,
            biome.temperature(),
            biome.downfall(),
            FOLIAGE_DEFAULT,
        )
    })
}

fn biome_dry_foliage_color(biome: Biome, _x: i32, _z: i32, colormaps: &Colormaps) -> [u8; 3] {
    biome.dry_foliage_color_override().unwrap_or_else(|| {
        sample_colormap(
            &colormaps.dry_foliage,
            biome.temperature(),
            biome.downfall(),
            DRY_FOLIAGE_DEFAULT,
        )
    })
}

fn biome_water_color(biome: Biome, _x: i32, _z: i32, _colormaps: &Colormaps) -> [u8; 3] {
    biome.water_color()
}

fn redstone_color(power: u8) -> [u8; 3] {
    let level = f32::from(power) / 15.0;
    let red = level * 0.6 + if power > 0 { 0.4 } else { 0.3 };
    let green = (level * level * 0.7 - 0.5).clamp(0.0, 1.0);
    let blue = (level * level * 0.6 - 0.7).clamp(0.0, 1.0);
    [
        (red * 255.0) as u8,
        (green * 255.0) as u8,
        (blue * 255.0) as u8,
    ]
}

fn stem_color(age: u8) -> [u8; 3] {
    [age * 32, 255 - age * 8, age * 4]
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcre_core::Vec3f;
    use mcre_resource_pack::lookup::BakedQuadEntry;
    use mcre_world::{Half, PropVal, SlabType, StairsShape};

    fn air() -> BlockState {
        Block::AIR.default_state()
    }

    fn quad(cullface: Option<Direction>) -> BakedQuadEntry {
        BakedQuadEntry {
            vertices: [
                Vec3f::new(0.0, 16.0, 0.0),
                Vec3f::new(0.0, 16.0, 16.0),
                Vec3f::new(16.0, 16.0, 16.0),
                Vec3f::new(16.0, 16.0, 0.0),
            ],
            uvs: [[0.0, 0.0]; 4],
            texture_id: 0,
            cullface,
            direction: cullface.unwrap_or(Direction::Up),
            tintindex: None,
            shade: 1.0,
            light_emission: 0,
            ambient_occlusion: true,
        }
    }

    fn flat_colormaps() -> Colormaps {
        fn flat(rgb: [u8; 3]) -> Box<[u8]> {
            let mut v = Vec::with_capacity(65536 * 3);
            for _ in 0..65536 {
                v.extend_from_slice(&rgb);
            }
            v.into_boxed_slice()
        }
        Colormaps {
            grass: flat([10, 20, 30]),
            foliage: flat([40, 50, 60]),
            dry_foliage: flat([70, 80, 90]),
        }
    }

    fn state_data(
        state: BlockState,
        quads: Vec<BakedQuadEntry>,
        colormaps: Colormaps,
    ) -> ResourcePackData {
        use mcre_resource_pack::lookup::{BlockRule, BlockStateEntry, BlockVariant};
        let id = u16::from(state) as usize;
        let mut entries = (0..=id)
            .map(|_| BlockStateEntry { rules: vec![] })
            .collect::<Vec<_>>();
        entries[id].rules = vec![BlockRule {
            variants: vec![BlockVariant { weight: 1, quads }],
        }];
        ResourcePackData {
            fluid_textures: Default::default(),
            chest_textures: Default::default(),
            font_textures: Default::default(),
            entity_textures: Default::default(),
            entries,
            particle_texture_ids: vec![],
            texture_table: vec![],
            culling: Default::default(),
            colormaps,
        }
    }

    fn stone_data(quads: Vec<BakedQuadEntry>) -> ResourcePackData {
        state_data(Block::STONE.default_state(), quads, flat_colormaps())
    }

    fn empty_data() -> ResourcePackData {
        ResourcePackData {
            fluid_textures: Default::default(),
            chest_textures: Default::default(),
            font_textures: Default::default(),
            entity_textures: Default::default(),
            entries: vec![],
            particle_texture_ids: vec![],
            texture_table: vec![],
            culling: Default::default(),
            colormaps: flat_colormaps(),
        }
    }

    fn fast_options() -> MeshOptions {
        MeshOptions {
            fast_leaves: true,
            ..MeshOptions::default()
        }
    }

    fn water_data(quads: Vec<BakedQuadEntry>) -> ResourcePackData {
        let water_id = u16::from(Block::WATER.default_state()) as usize;
        let mut entries = (0..=water_id)
            .map(|_| mcre_resource_pack::lookup::BlockStateEntry { rules: vec![] })
            .collect::<Vec<_>>();
        entries[water_id].rules = vec![mcre_resource_pack::lookup::BlockRule {
            variants: vec![mcre_resource_pack::lookup::BlockVariant { weight: 1, quads }],
        }];
        ResourcePackData {
            fluid_textures: Default::default(),
            chest_textures: Default::default(),
            font_textures: Default::default(),
            entity_textures: Default::default(),
            entries,
            particle_texture_ids: vec![],
            texture_table: vec![],
            culling: Default::default(),
            colormaps: flat_colormaps(),
        }
    }

    fn stone_data_multi(variants: Vec<BakedQuadEntry>) -> ResourcePackData {
        use mcre_resource_pack::lookup::{BlockRule, BlockVariant};
        let stone_id = u16::from(Block::STONE.default_state()) as usize;
        let mut entries = (0..=stone_id)
            .map(|_| mcre_resource_pack::lookup::BlockStateEntry { rules: vec![] })
            .collect::<Vec<_>>();
        entries[stone_id].rules = vec![BlockRule {
            variants: variants
                .into_iter()
                .map(|quads| BlockVariant {
                    weight: 1,
                    quads: vec![quads],
                })
                .collect(),
        }];
        ResourcePackData {
            fluid_textures: Default::default(),
            chest_textures: Default::default(),
            font_textures: Default::default(),
            entity_textures: Default::default(),
            entries,
            particle_texture_ids: vec![],
            texture_table: vec![],
            culling: Default::default(),
            colormaps: flat_colormaps(),
        }
    }

    fn test_atlas() -> Atlas {
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

    #[test]
    fn culls_faces_against_full_cube_neighbors() {
        let data = empty_data();
        let stone = Block::STONE.default_state();

        assert!(!should_render_face(
            &data,
            stone,
            stone,
            Direction::Up,
            MeshOptions::default()
        ));
        assert!(should_render_face(
            &data,
            stone,
            air(),
            Direction::Up,
            MeshOptions::default()
        ));
        assert!(!should_render_face(
            &data,
            stone,
            bottom_slab(),
            Direction::Up,
            MeshOptions::default()
        ));
        assert!(should_render_face(
            &data,
            stone,
            top_slab(),
            Direction::Up,
            MeshOptions::default()
        ));
    }

    #[test]
    fn transparent_same_block_faces_are_culled() {
        let data = empty_data();
        let glass = Block::GLASS.default_state();
        let water = Block::WATER.default_state();
        let lava = Block::LAVA.default_state();

        assert!(!should_render_face(
            &data,
            glass,
            glass,
            Direction::North,
            MeshOptions::default()
        ));
        assert!(should_render_face(
            &data,
            glass,
            air(),
            Direction::North,
            MeshOptions::default()
        ));
        assert!(should_render_face(
            &data,
            glass,
            water,
            Direction::North,
            MeshOptions::default()
        ));

        assert!(!should_render_face(
            &data,
            water,
            water,
            Direction::Up,
            MeshOptions::default()
        ));
        assert!(should_render_face(
            &data,
            water,
            lava,
            Direction::Up,
            MeshOptions::default()
        ));

        let ice = Block::ICE.default_state();
        let frosted = Block::FROSTED_ICE.default_state();
        assert!(!should_render_face(
            &data,
            ice,
            frosted,
            Direction::Up,
            MeshOptions::default()
        ));
        assert!(!should_render_face(
            &data,
            frosted,
            ice,
            Direction::Up,
            MeshOptions::default()
        ));
        assert!(should_render_face(
            &data,
            ice,
            air(),
            Direction::Up,
            MeshOptions::default()
        ));
    }

    #[test]
    fn vanilla_skip_rendering_powder_snow() {
        let data = empty_data();
        let options = MeshOptions::default();
        let snow = Block::POWDER_SNOW.default_state();
        let stone = Block::STONE.default_state();

        // PowderSnowBlock.skipRendering: cull only against powder snow.
        for direction in Direction::ALL {
            assert!(
                !should_render_face(&data, snow, snow, direction, options),
                "powder snow vs powder snow {direction:?}"
            );
        }
        assert!(should_render_face(
            &data,
            snow,
            air(),
            Direction::Up,
            options
        ));
        assert!(should_render_face(
            &data,
            snow,
            Block::GLASS.default_state(),
            Direction::North,
            options
        ));
        assert!(!should_render_face(
            &data,
            snow,
            stone,
            Direction::Up,
            options
        ));

        // Empty occlusion shape: powder snow hides no neighbor faces.
        assert!(should_render_face(
            &data,
            stone,
            snow,
            Direction::Up,
            options
        ));
    }

    #[test]
    fn vanilla_skip_rendering_copper_grate() {
        let data = empty_data();
        let options = MeshOptions::default();
        let grate = Block::COPPER_GRATE.default_state();
        let waterlogged = Block::COPPER_GRATE
            .state(&[PropVal::Waterlogged(true)])
            .expect("grate waterlogged");
        let exposed = Block::EXPOSED_COPPER_GRATE.default_state();
        let stone = Block::STONE.default_state();

        // HalfTransparentBlock.skipRendering: cull only against the same
        // grate block, regardless of state.
        for direction in Direction::ALL {
            assert!(
                !should_render_face(&data, grate, grate, direction, options),
                "grate vs grate {direction:?}"
            );
            assert!(
                !should_render_face(&data, grate, waterlogged, direction, options),
                "grate vs waterlogged grate {direction:?}"
            );
        }
        assert!(should_render_face(
            &data,
            grate,
            exposed,
            Direction::Up,
            options
        ));
        assert!(should_render_face(
            &data,
            grate,
            air(),
            Direction::Up,
            options
        ));
        assert!(!should_render_face(
            &data,
            grate,
            stone,
            Direction::Up,
            options
        ));

        // Empty occlusion shape: grates hide no neighbor faces.
        assert!(should_render_face(
            &data,
            stone,
            grate,
            Direction::Up,
            options
        ));
    }

    fn east_straight_stair() -> BlockState {
        (u16::from(Block::OAK_STAIRS.min_state())..=u16::from(Block::OAK_STAIRS.max_state()))
            .map(BlockState::from)
            .find(|state| {
                state.facing() == Direction::East
                    && state.half() == Half::Bottom
                    && state.stairs_shape() == StairsShape::Straight
            })
            .unwrap()
    }

    fn bottom_slab() -> BlockState {
        (u16::from(Block::OAK_SLAB.min_state())..=u16::from(Block::OAK_SLAB.max_state()))
            .map(BlockState::from)
            .find(|state| state.slab_type() == SlabType::Bottom)
            .unwrap()
    }

    fn top_slab() -> BlockState {
        (u16::from(Block::OAK_SLAB.min_state())..=u16::from(Block::OAK_SLAB.max_state()))
            .map(BlockState::from)
            .find(|state| state.slab_type() == SlabType::Top)
            .unwrap()
    }

    fn ns_fence() -> BlockState {
        (u16::from(Block::OAK_FENCE.min_state())..=u16::from(Block::OAK_FENCE.max_state()))
            .map(BlockState::from)
            .find(|state| {
                state.is_north()
                    && state.is_south()
                    && !state.is_east()
                    && !state.is_west()
                    && !state.is_waterlogged()
            })
            .unwrap()
    }

    #[test]
    fn empty_occlusion_shapes_render_against_partial_neighbors() {
        let data = empty_data();
        let options = MeshOptions::default();
        let stone = Block::STONE.default_state();
        let fence = Block::OAK_FENCE.default_state();
        let stair = east_straight_stair();
        let bot = bottom_slab();

        // Vanilla renders the face when the state's own face shape is empty,
        // even against partial occluders that do not reach the unit cube.
        for state in [
            Block::POWDER_SNOW.default_state(),
            Block::GLASS.default_state(),
            Block::COPPER_GRATE.default_state(),
        ] {
            assert!(should_render_face(
                &data,
                state,
                fence,
                Direction::Down,
                options
            ));
            assert!(should_render_face(
                &data,
                state,
                stair,
                Direction::Up,
                options
            ));
            assert!(should_render_face(
                &data,
                state,
                stair,
                Direction::Down,
                options
            ));
            assert!(!should_render_face(
                &data,
                state,
                stone,
                Direction::Down,
                options
            ));
            assert!(!should_render_face(
                &data,
                state,
                bot,
                Direction::Up,
                options
            ));
        }

        // A bottom slab's up face does not reach its cell boundary: render.
        assert!(should_render_face(
            &data,
            bot,
            fence,
            Direction::Up,
            options
        ));

        // Full cube faces still render against the fence post footprint.
        assert!(should_render_face(
            &data,
            stone,
            fence,
            Direction::Down,
            options
        ));

        // The lower bottom slab's up face culls against the upper one, whose
        // down face is the unit-cube singleton.
        assert!(!should_render_face(&data, bot, bot, Direction::Up, options));
    }

    fn probe_states() -> [BlockState; 13] {
        [
            BlockState::from(1),
            BlockState::from(661),
            BlockState::from(268),
            BlockState::from(5534),
            BlockState::from(8589),
            BlockState::from(8668),
            BlockState::from(8801),
            BlockState::from(10067),
            BlockState::from(13082),
            top_slab(),
            bottom_slab(),
            Block::POWDER_SNOW.default_state(),
            Block::COPPER_GRATE.default_state(),
        ]
    }

    /// Vanilla `getFaceOcclusionShape` per direction: `c` = unit-cube
    /// singleton, `e` = empty singleton, `o` = any other shape.
    const PROBE_FACE_KINDS: [&str; 13] = [
        "cccccc", "eeeeee", "eeeeee", "oooooo", "ceoooo", "ooeeee", "eeeeee", "eeeeee", "eeeeee",
        "ecoooo", "ceoooo", "eeeeee", "eeeeee",
    ];

    /// Vanilla `Block.shouldRenderFace` masks from the same probe: bit `i`
    /// set = direction with 3D data value `i` (down, up, north, south, west,
    /// east) renders.
    const PROBE_SHOULD_RENDER: [[u8; 13]; 13] = [
        [
            0x00, 0x3f, 0x3f, 0x2d, 0x3d, 0x3f, 0x3f, 0x3f, 0x3f, 0x3e, 0x3d, 0x3f, 0x3f,
        ],
        [
            0x00, 0x00, 0x3f, 0x3f, 0x3d, 0x3f, 0x3f, 0x3f, 0x3f, 0x3e, 0x3d, 0x3f, 0x3f,
        ],
        [
            0x00, 0x3f, 0x3f, 0x3f, 0x3d, 0x3f, 0x3f, 0x3f, 0x3f, 0x3e, 0x3d, 0x3f, 0x3f,
        ],
        [
            0x00, 0x3f, 0x3f, 0x21, 0x3d, 0x3f, 0x3f, 0x3f, 0x3f, 0x3e, 0x2d, 0x3f, 0x3f,
        ],
        [
            0x00, 0x3f, 0x3f, 0x03, 0x01, 0x3f, 0x3f, 0x3f, 0x3f, 0x3e, 0x01, 0x3f, 0x3f,
        ],
        [
            0x00, 0x3f, 0x3f, 0x3d, 0x3d, 0x3c, 0x3f, 0x3f, 0x3f, 0x3e, 0x3d, 0x3f, 0x3f,
        ],
        [
            0x00, 0x3f, 0x3f, 0x3f, 0x3d, 0x3f, 0x3f, 0x3f, 0x3f, 0x3e, 0x3d, 0x3f, 0x3f,
        ],
        [
            0x00, 0x3f, 0x3f, 0x3f, 0x3d, 0x3f, 0x3f, 0x3c, 0x3f, 0x3e, 0x3d, 0x3f, 0x3f,
        ],
        [
            0x00, 0x3f, 0x3f, 0x3f, 0x3d, 0x3f, 0x3f, 0x3f, 0x3f, 0x3e, 0x3d, 0x3f, 0x3f,
        ],
        [
            0x00, 0x3f, 0x3f, 0x2d, 0x3d, 0x3f, 0x3f, 0x3f, 0x3f, 0x02, 0x3d, 0x3f, 0x3f,
        ],
        [
            0x00, 0x3f, 0x3f, 0x03, 0x3d, 0x3f, 0x3f, 0x3f, 0x3f, 0x3e, 0x01, 0x3f, 0x3f,
        ],
        [
            0x00, 0x3f, 0x3f, 0x3f, 0x3d, 0x3f, 0x3f, 0x3f, 0x3f, 0x3e, 0x3d, 0x00, 0x3f,
        ],
        [
            0x00, 0x3f, 0x3f, 0x3f, 0x3d, 0x3f, 0x3f, 0x3f, 0x3f, 0x3e, 0x3d, 0x3f, 0x00,
        ],
    ];

    #[test]
    fn face_shapes_and_culling_match_vanilla_probe() {
        let data = empty_data();
        let options = MeshOptions::default();
        let directions = [
            Direction::Down,
            Direction::Up,
            Direction::North,
            Direction::South,
            Direction::West,
            Direction::East,
        ];
        let states = probe_states();

        for (i, state) in states.iter().enumerate() {
            let boxes = state.occlusion_boxes();
            for (dir_idx, direction) in directions.iter().enumerate() {
                let empty = face_boxes(boxes, *direction).is_empty();
                let cube = face_shape_is_cube(boxes, *direction);
                let expected = PROBE_FACE_KINDS[i].as_bytes()[dir_idx];
                let actual = match (empty, cube) {
                    (true, false) => b'e',
                    (false, true) => b'c',
                    (false, false) => b'o',
                    (true, true) => unreachable!(),
                };
                assert_eq!(
                    actual,
                    expected,
                    "face kind of state {} toward {direction:?}",
                    u16::from(*state)
                );
            }
        }

        for (i, state) in states.iter().enumerate() {
            for (j, neighbor) in states.iter().enumerate() {
                for (dir_idx, direction) in directions.iter().enumerate() {
                    let expected = PROBE_SHOULD_RENDER[i][j] >> dir_idx & 1 == 1;
                    assert_eq!(
                        should_render_face(&data, *state, *neighbor, *direction, options),
                        expected,
                        "state {} neighbor {} toward {direction:?}",
                        u16::from(*state),
                        u16::from(*neighbor)
                    );
                }
            }
        }
    }

    #[test]
    fn shape_occlusion_matches_vanilla_probes() {
        let data = empty_data();
        let options = MeshOptions::default();
        let stone = Block::STONE.default_state();
        let stair = east_straight_stair();
        let bot = bottom_slab();
        let top = top_slab();
        let fence = ns_fence();
        let plain_fence = Block::OAK_FENCE.default_state();

        assert!(should_render_face(&data, bot, top, Direction::Up, options));
        assert!(should_render_face(
            &data,
            top,
            bot,
            Direction::Down,
            options
        ));
        assert!(should_render_face(
            &data,
            stair,
            stair,
            Direction::East,
            options
        ));
        assert!(!should_render_face(
            &data,
            stair,
            stair,
            Direction::West,
            options
        ));
        assert!(should_render_face(
            &data,
            stone,
            stair,
            Direction::Down,
            options
        ));
        assert!(!should_render_face(
            &data,
            stair,
            stone,
            Direction::Up,
            options
        ));
        assert!(!should_render_face(
            &data,
            fence,
            fence,
            Direction::North,
            options
        ));
        assert!(should_render_face(
            &data,
            plain_fence,
            plain_fence,
            Direction::North,
            options
        ));
        assert!(!should_render_face(&data, bot, bot, Direction::Up, options));
        assert!(should_render_face(
            &data,
            stone,
            fence,
            Direction::North,
            options
        ));
    }

    #[test]
    fn liquid_faces_skip_matching_fluid_neighbors() {
        let data = empty_data();
        let water = Block::WATER.default_state();
        let fence = (u16::from(Block::OAK_FENCE.min_state())
            ..=u16::from(Block::OAK_FENCE.max_state()))
            .map(BlockState::from)
            .find(|state| state.is_waterlogged())
            .unwrap();

        assert!(!should_render_face(
            &data,
            water,
            fence,
            Direction::Up,
            MeshOptions::default()
        ));
        assert!(should_render_face(
            &data,
            fence,
            water,
            Direction::Up,
            MeshOptions::default()
        ));
    }

    #[test]
    fn fast_leaves_culls_leaf_to_leaf_faces() {
        let data = empty_data();
        let oak_leaves = Block::OAK_LEAVES.default_state();
        let birch_leaves = Block::BIRCH_LEAVES.default_state();
        let grass = Block::SHORT_GRASS.default_state();

        assert!(!should_render_face(
            &data,
            oak_leaves,
            birch_leaves,
            Direction::Up,
            fast_options()
        ));
        assert!(!should_render_face(
            &data,
            oak_leaves,
            oak_leaves,
            Direction::North,
            fast_options()
        ));
        assert!(should_render_face(
            &data,
            oak_leaves,
            grass,
            Direction::Up,
            fast_options()
        ));
        assert!(should_render_face(
            &data,
            grass,
            oak_leaves,
            Direction::Up,
            fast_options()
        ));
    }

    #[test]
    fn cross_block_bars_follow_connection_props() {
        let data = empty_data();
        let iron = Block::IRON_BARS.default_state();
        let copper = Block::COPPER_BARS.default_state();

        assert!(should_render_face(
            &data,
            iron,
            copper,
            Direction::North,
            MeshOptions::default()
        ));
        assert!(!should_render_face(
            &data,
            iron,
            copper,
            Direction::Up,
            MeshOptions::default()
        ));

        let iron_north = (u16::from(Block::IRON_BARS.min_state())
            ..=u16::from(Block::IRON_BARS.max_state()))
            .map(BlockState::from)
            .find(|state| {
                state.is_north() && !state.is_south() && !state.is_east() && !state.is_west()
            })
            .unwrap();
        let copper_south = (u16::from(Block::COPPER_BARS.min_state())
            ..=u16::from(Block::COPPER_BARS.max_state()))
            .map(BlockState::from)
            .find(|state| {
                state.is_south() && !state.is_north() && !state.is_east() && !state.is_west()
            })
            .unwrap();
        assert!(!should_render_face(
            &data,
            iron_north,
            copper_south,
            Direction::North,
            MeshOptions::default()
        ));
        assert!(should_render_face(
            &data,
            iron_north,
            copper_south,
            Direction::East,
            MeshOptions::default()
        ));
    }

    #[test]
    fn double_sided_liquids_duplicate_indices() {
        let data = water_data(vec![quad(Some(Direction::Up))]);
        let atlas = test_atlas();
        let mut chunk = Chunk::new();
        chunk.set_block(0, 0, 0, Block::WATER.default_state());
        let world = World::new(0);

        let mesh = mesh_chunk(
            &data,
            &atlas,
            &chunk,
            &world,
            [0, 0],
            LightMode::Flat,
            MeshOptions::default(),
        );
        assert_eq!(mesh.indices.len(), 12);

        let single = MeshOptions {
            double_sided_liquids: false,
            ..MeshOptions::default()
        };
        let mesh = mesh_chunk(
            &data,
            &atlas,
            &chunk,
            &world,
            [0, 0],
            LightMode::Flat,
            single,
        );
        assert_eq!(mesh.indices.len(), 6);
    }

    fn chest_data() -> ResourcePackData {
        let mut data = empty_data();
        data.texture_table = vec![
            "minecraft:entity/chest/normal".into(),
            "minecraft:entity/chest/normal_left".into(),
            "minecraft:entity/chest/normal_right".into(),
        ];
        data.chest_textures.normal = mcre_resource_pack::lookup::ChestTextures {
            single: 0,
            left: 1,
            right: 2,
        };
        data
    }

    fn chest_atlas() -> Atlas {
        let mut atlas = test_atlas();
        atlas.regions = vec![AtlasRegion {
            x: 0,
            y: 0,
            width: 64,
            height: 64,
        }];
        atlas.has_alpha = vec![false];
        atlas
    }

    fn chest_state(facing: Direction) -> BlockState {
        Block::CHEST
            .state(&[mcre_world::PropVal::Facing(facing)])
            .expect("chest facing")
    }

    fn mesh_chest_at(facing: Direction) -> Mesh {
        let mut chunk = Chunk::new();
        chunk.set_block(0, 0, 0, chest_state(facing));
        mesh_chunk(
            &chest_data(),
            &chest_atlas(),
            &chunk,
            &World::new(0),
            [0, 0],
            LightMode::Flat,
            MeshOptions::default(),
        )
    }

    #[test]
    fn chest_emits_bottom_lid_and_lock_geometry() {
        let mesh = mesh_chest_at(Direction::South);
        assert_eq!(mesh.vertices.len(), 72);
        assert_eq!(mesh.indices.len(), 108);
        assert_eq!(mesh.opaque_meta.len(), 18);
        assert!(mesh.translucent_indices.is_empty());
    }

    #[test]
    fn chest_facing_rotates_geometry() {
        let south = mesh_chest_at(Direction::South);
        let south_front = south
            .vertices
            .iter()
            .map(|vertex| vertex.position[2])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!((south_front - 1.0).abs() < 1e-6);

        let east = mesh_chest_at(Direction::East);
        let east_front = east
            .vertices
            .iter()
            .map(|vertex| vertex.position[0])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!((east_front - 1.0).abs() < 1e-6);
        let east_max_z = east
            .vertices
            .iter()
            .map(|vertex| vertex.position[2])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(east_max_z < 1.0);
    }

    #[test]
    fn double_chest_halves_use_half_models() {
        for (chest_type, expected) in [
            (mcre_world::TypePropVal::Left, Direction::West),
            (mcre_world::TypePropVal::Right, Direction::East),
        ] {
            let mut chunk = Chunk::new();
            let half = Block::CHEST
                .state(&[
                    mcre_world::PropVal::Facing(Direction::South),
                    mcre_world::PropVal::Type(chest_type),
                ])
                .expect("double chest half");
            chunk.set_block(0, 0, 0, half);
            let mesh = mesh_chunk(
                &chest_data(),
                &chest_atlas(),
                &chunk,
                &World::new(0),
                [0, 0],
                LightMode::Flat,
                MeshOptions::default(),
            );
            assert_eq!(mesh.vertices.len(), 60);
            assert_eq!(mesh.indices.len(), 90);
            assert!(
                mesh.opaque_meta
                    .iter()
                    .all(|meta| meta.facing != facing_of(expected)),
                "seam face {expected:?} must be hidden"
            );
        }
    }

    #[test]
    fn chest_texture_must_match_the_vanilla_path() {
        let mut data = chest_data();
        data.texture_table = vec!["minecraft:block/stone".into()];
        let mut chunk = Chunk::new();
        chunk.set_block(0, 0, 0, chest_state(Direction::South));
        let mesh = mesh_chunk(
            &data,
            &chest_atlas(),
            &chunk,
            &World::new(0),
            [0, 0],
            LightMode::Flat,
            MeshOptions::default(),
        );
        assert!(mesh.is_empty());
    }

    fn sign_entity(
        front: &[&str],
        back: &[&str],
        color: &str,
        glowing: bool,
    ) -> mcre_world::BlockEntity {
        use mcre_anvil::nbt::Tag;
        let side = |lines: &[&str]| {
            Tag::Compound(vec![
                (
                    "messages".into(),
                    Tag::List(
                        lines
                            .iter()
                            .map(|line| Tag::String((*line).into()))
                            .collect(),
                    ),
                ),
                ("color".into(), Tag::String(color.into())),
                (
                    "has_glowing_text".into(),
                    Tag::Byte(if glowing { 1 } else { 0 }),
                ),
            ])
        };
        let root = Tag::Compound(vec![
            ("front_text".into(), side(front)),
            ("back_text".into(), side(back)),
        ]);
        mcre_world::BlockEntity::new(
            "minecraft:sign".into(),
            (0, 0, 0),
            mcre_anvil::nbt::write_named("", &root),
        )
    }

    fn sign_data() -> ResourcePackData {
        let mut data = empty_data();
        data.texture_table = vec!["minecraft:font/ascii".into()];
        data.font_textures.ascii = 0;
        data
    }

    fn sign_atlas() -> Atlas {
        let mut atlas = test_atlas();
        atlas.width = 160;
        atlas.height = 160;
        atlas.pixels = vec![0; 160 * 160 * 4];
        let cell_x = 16 + 8;
        let cell_y = 16 + 32;
        for y in 0..8 {
            for x in 0..6 {
                let offset = ((cell_y + y) * 160 + cell_x + x) * 4;
                atlas.pixels[offset..offset + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
        atlas.regions = vec![AtlasRegion {
            x: 16,
            y: 16,
            width: 128,
            height: 128,
        }];
        atlas.has_alpha = vec![true];
        atlas
    }

    fn sign_state(rotation: u8) -> BlockState {
        Block::OAK_SIGN
            .state(&[mcre_world::PropVal::Rotation(rotation)])
            .expect("sign rotation")
    }

    fn mesh_sign(data: &ResourcePackData, atlas: &Atlas, chunk: &Chunk) -> Mesh {
        mesh_chunk(
            data,
            atlas,
            chunk,
            &World::new(0),
            [0, 0],
            LightMode::Flat,
            MeshOptions::default(),
        )
    }

    fn entity(id: &str, yaw: f32, variant: &str) -> mcre_world::WorldEntity {
        let root = Tag::Compound(vec![
            ("id".into(), Tag::String(id.into())),
            (
                "Rotation".into(),
                Tag::List(vec![Tag::Float(yaw), Tag::Float(0.0)]),
            ),
            ("variant".into(), Tag::String(variant.into())),
        ]);
        mcre_world::WorldEntity::new(
            id.into(),
            [8.0, 64.0, 8.0],
            mcre_anvil::nbt::write_named("", &root),
        )
    }

    fn pig_data() -> ResourcePackData {
        let mut data = empty_data();
        data.texture_table = vec!["minecraft:entity/pig/pig_temperate".into()];
        data.entity_textures.pig_temperate = 0;
        data
    }

    fn cow_data() -> ResourcePackData {
        let mut data = empty_data();
        data.texture_table = vec!["minecraft:entity/cow/cow_temperate".into()];
        data.entity_textures.cow_temperate = 0;
        data
    }

    fn chicken_data() -> ResourcePackData {
        let mut data = empty_data();
        data.texture_table = vec!["minecraft:entity/chicken/chicken_temperate".into()];
        data.entity_textures.chicken_temperate = 0;
        data
    }

    fn creeper_data() -> ResourcePackData {
        let mut data = empty_data();
        data.texture_table = vec!["minecraft:entity/creeper/creeper".into()];
        data.entity_textures.creeper = 0;
        data
    }

    fn sheep_data() -> ResourcePackData {
        let mut data = empty_data();
        data.texture_table = vec![
            "minecraft:entity/sheep/sheep".into(),
            "minecraft:entity/sheep/sheep_wool".into(),
        ];
        data.entity_textures.sheep = 0;
        data.entity_textures.sheep_wool = 1;
        data
    }

    fn zombie_data() -> ResourcePackData {
        let mut data = empty_data();
        data.texture_table = vec!["minecraft:entity/zombie/zombie".into()];
        data.entity_textures.zombie = 0;
        data
    }

    fn drowned_data() -> ResourcePackData {
        let mut data = empty_data();
        data.texture_table = vec!["minecraft:entity/zombie/drowned".into()];
        data.entity_textures.drowned = 0;
        data
    }

    fn skeleton_data() -> ResourcePackData {
        let mut data = empty_data();
        data.texture_table = vec!["minecraft:entity/skeleton/skeleton".into()];
        data.entity_textures.skeleton = 0;
        data
    }

    fn wolf_data() -> ResourcePackData {
        let mut data = empty_data();
        data.texture_table = vec!["minecraft:entity/wolf/wolf".into()];
        data.entity_textures.wolf_pale = 0;
        data
    }

    fn parrot_data() -> ResourcePackData {
        let mut data = empty_data();
        data.texture_table = vec!["minecraft:entity/parrot/parrot_red_blue".into()];
        data.entity_textures.parrot_red_blue = 0;
        data
    }

    fn bat_data() -> ResourcePackData {
        let mut data = empty_data();
        data.texture_table = vec!["minecraft:entity/bat/bat".into()];
        data.entity_textures.bat = 0;
        data
    }

    fn bee_data() -> ResourcePackData {
        let mut data = empty_data();
        data.texture_table = vec!["minecraft:entity/bee/bee".into()];
        data.entity_textures.bee = 0;
        data
    }

    fn enderman_data() -> ResourcePackData {
        let mut data = empty_data();
        data.texture_table = vec!["minecraft:entity/enderman/enderman".into()];
        data.entity_textures.enderman = 0;
        data
    }

    fn squid_data() -> ResourcePackData {
        let mut data = empty_data();
        data.texture_table = vec!["minecraft:entity/squid/squid".into()];
        data.entity_textures.squid = 0;
        data
    }

    fn parrot_entity(variant: i32) -> mcre_world::WorldEntity {
        let root = Tag::Compound(vec![
            ("id".into(), Tag::String("minecraft:parrot".into())),
            (
                "Rotation".into(),
                Tag::List(vec![Tag::Float(0.0), Tag::Float(0.0)]),
            ),
            ("Variant".into(), Tag::Int(variant)),
        ]);
        mcre_world::WorldEntity::new(
            "minecraft:parrot".into(),
            [8.0, 64.0, 8.0],
            mcre_anvil::nbt::write_named("", &root),
        )
    }

    fn wolf_atlas() -> Atlas {
        let mut atlas = test_atlas();
        atlas.width = 64;
        atlas.height = 64;
        atlas.pixels = vec![255; 64 * 64 * 4];
        atlas.regions = vec![
            AtlasRegion {
                x: 0,
                y: 0,
                width: 64,
                height: 32,
            },
            AtlasRegion {
                x: 0,
                y: 32,
                width: 64,
                height: 32,
            },
        ];
        atlas.has_alpha = vec![true, true];
        atlas
    }

    fn parrot_atlas() -> Atlas {
        let mut atlas = test_atlas();
        atlas.width = 32;
        atlas.height = 64;
        atlas.pixels = vec![255; 32 * 64 * 4];
        atlas.regions = vec![
            AtlasRegion {
                x: 0,
                y: 0,
                width: 32,
                height: 32,
            },
            AtlasRegion {
                x: 0,
                y: 32,
                width: 32,
                height: 32,
            },
        ];
        atlas.has_alpha = vec![true, true];
        atlas
    }

    fn sheep_atlas() -> Atlas {
        let mut atlas = test_atlas();
        atlas.width = 64;
        atlas.height = 64;
        atlas.pixels = vec![255; 64 * 64 * 4];
        atlas.regions = vec![
            AtlasRegion {
                x: 0,
                y: 0,
                width: 64,
                height: 32,
            },
            AtlasRegion {
                x: 0,
                y: 32,
                width: 64,
                height: 32,
            },
        ];
        atlas.has_alpha = vec![true, true];
        atlas
    }

    fn entity_atlas() -> Atlas {
        let mut atlas = test_atlas();
        atlas.width = 64;
        atlas.height = 64;
        atlas.pixels = vec![255; 64 * 64 * 4];
        atlas.regions = vec![AtlasRegion {
            x: 0,
            y: 0,
            width: 64,
            height: 64,
        }];
        atlas.has_alpha = vec![true];
        atlas
    }

    fn mesh_entity_with_atlas(data: &ResourcePackData, atlas: &Atlas, chunk: &Chunk) -> Mesh {
        mesh_chunk(
            data,
            atlas,
            chunk,
            &World::new(0),
            [0, 0],
            LightMode::Flat,
            MeshOptions::default(),
        )
    }

    fn mesh_entity(data: &ResourcePackData, chunk: &Chunk) -> Mesh {
        mesh_entity_with_atlas(data, &entity_atlas(), chunk)
    }

    fn mesh_bounds(mesh: &Mesh) -> ([f32; 3], [f32; 3]) {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for vertex in &mesh.vertices {
            for axis in 0..3 {
                min[axis] = min[axis].min(vertex.position[axis]);
                max[axis] = max[axis].max(vertex.position[axis]);
            }
        }
        (min, max)
    }

    #[test]
    fn sign_text_centers_ascii_glyphs_on_the_front() {
        let mut chunk = Chunk::new();
        chunk.set_block(0, 0, 0, sign_state(0));
        assert!(chunk.set_block_entity(sign_entity(&["A"], &[], "black", false)));

        let mesh = mesh_sign(&sign_data(), &sign_atlas(), &chunk);
        assert_eq!(mesh.translucent_indices.len(), 6);
        assert_eq!(mesh.translucent_meta.len(), 1);
        assert_eq!(mesh.translucent_meta[0].facing, FACING_UNASSIGNED);

        let min_x = mesh
            .vertices
            .iter()
            .map(|vertex| vertex.position[0])
            .fold(f32::INFINITY, f32::min);
        let max_x = mesh
            .vertices
            .iter()
            .map(|vertex| vertex.position[0])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!((min_x - (0.5 - 3.5 / 96.0)).abs() < 1e-5);
        assert!((max_x - (0.5 + 4.5 / 96.0)).abs() < 1e-5);
        let z = mesh.vertices[0].position[2];
        assert!((z - (0.5 + 0.046_666_667 + 0.001)).abs() < 1e-5);
        assert!((mesh.vertices[0].uv[0] - 24.0 / 160.0).abs() < 1e-6);
        assert!((mesh.vertices[0].uv[1] - 48.0 / 160.0).abs() < 1e-6);
        assert_eq!(mesh.vertices[0].color, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn sign_text_back_is_flipped_to_the_rear_plane() {
        let mut chunk = Chunk::new();
        chunk.set_block(0, 0, 0, sign_state(0));
        assert!(chunk.set_block_entity(sign_entity(&[], &["A"], "black", false)));

        let mesh = mesh_sign(&sign_data(), &sign_atlas(), &chunk);
        assert_eq!(mesh.translucent_indices.len(), 6);
        let min_x = mesh
            .vertices
            .iter()
            .map(|vertex| vertex.position[0])
            .fold(f32::INFINITY, f32::min);
        let max_x = mesh
            .vertices
            .iter()
            .map(|vertex| vertex.position[0])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!((min_x - (0.5 - 4.5 / 96.0)).abs() < 1e-5);
        assert!((max_x - (0.5 + 3.5 / 96.0)).abs() < 1e-5);
        let z = mesh.vertices[0].position[2];
        assert!((z - (0.5 - 0.046_666_667 - 0.001)).abs() < 1e-5);
    }

    #[test]
    fn wall_sign_text_uses_facing_and_wall_offset() {
        let mut chunk = Chunk::new();
        chunk.set_block(0, 0, 0, Block::OAK_WALL_SIGN.default_state());
        assert!(chunk.set_block_entity(sign_entity(&["A"], &[], "black", false)));

        let mesh = mesh_sign(&sign_data(), &sign_atlas(), &chunk);
        assert_eq!(mesh.translucent_indices.len(), 6);
        let z = mesh.vertices[0].position[2];
        assert!((z - (0.5 + 0.4375 - 0.046_666_667 - 0.001)).abs() < 1e-5);
    }

    #[test]
    fn glowing_sign_text_uses_bright_color_and_full_light() {
        let mut chunk = Chunk::new();
        chunk.set_block(0, 0, 0, sign_state(0));
        assert!(chunk.set_block_entity(sign_entity(&["A"], &[], "red", true)));

        let mesh = mesh_sign(&sign_data(), &sign_atlas(), &chunk);
        assert_eq!(mesh.vertices[0].color, [1.0, 0.0, 0.0]);
        assert_eq!(mesh.vertices[0].light, [15.0, 15.0]);
    }

    #[test]
    fn sign_text_dark_colors_match_vanilla_scale() {
        let mut chunk = Chunk::new();
        chunk.set_block(0, 0, 0, sign_state(0));
        assert!(chunk.set_block_entity(sign_entity(&["A"], &[], "orange", false)));

        let mesh = mesh_sign(&sign_data(), &sign_atlas(), &chunk);
        let color = mesh.vertices[0].color;
        assert!((color[0] - 102.0 / 255.0).abs() < 1e-6);
        assert!((color[1] - 41.0 / 255.0).abs() < 1e-6);
        assert!((color[2] - 12.0 / 255.0).abs() < 1e-6);
    }

    #[test]
    fn sign_text_strips_formatting_codes() {
        let mut chunk = Chunk::new();
        chunk.set_block(0, 0, 0, sign_state(0));
        assert!(chunk.set_block_entity(sign_entity(&["§cA"], &[], "black", false)));

        let mesh = mesh_sign(&sign_data(), &sign_atlas(), &chunk);
        assert_eq!(mesh.translucent_indices.len(), 6);
        let min_x = mesh
            .vertices
            .iter()
            .map(|vertex| vertex.position[0])
            .fold(f32::INFINITY, f32::min);
        assert!((min_x - (0.5 - 3.5 / 96.0)).abs() < 1e-5);
    }

    #[test]
    fn sign_text_uses_vanilla_space_advance() {
        let mut chunk = Chunk::new();
        chunk.set_block(0, 0, 0, sign_state(0));
        assert!(chunk.set_block_entity(sign_entity(&["A A"], &[], "black", false)));

        let mesh = mesh_sign(&sign_data(), &sign_atlas(), &chunk);
        assert_eq!(mesh.translucent_indices.len(), 12);
        let min_x = mesh
            .vertices
            .iter()
            .map(|vertex| vertex.position[0])
            .fold(f32::INFINITY, f32::min);
        let max_x = mesh
            .vertices
            .iter()
            .map(|vertex| vertex.position[0])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!((min_x - (0.5 - 9.0 / 96.0)).abs() < 1e-5);
        assert!((max_x - (0.5 + 10.0 / 96.0)).abs() < 1e-5);
    }

    #[test]
    fn pig_entity_bakes_model_into_chunk_mesh() {
        let mut chunk = Chunk::new();
        assert!(chunk.set_entity(entity("minecraft:pig", 0.0, "minecraft:temperate")));

        let mesh = mesh_entity(&pig_data(), &chunk);
        assert_eq!(mesh.opaque_meta.len(), 42);
        assert_eq!(mesh.indices.len(), 42 * 6);
        let (min, max) = mesh_bounds(&mesh);
        assert!((min[0] - 7.6875).abs() < 1e-4);
        assert!((max[0] - 8.3125).abs() < 1e-4);
        assert!((min[1] - 64.001).abs() < 1e-4);
        assert!((max[1] - 65.001).abs() < 1e-4);
        assert!((min[2] - 7.4375).abs() < 1e-4);
        assert!((max[2] - 8.9375).abs() < 1e-4);
    }

    #[test]
    fn pig_entity_yaw_rotates_the_model() {
        let mut chunk = Chunk::new();
        assert!(chunk.set_entity(entity("minecraft:pig", 90.0, "minecraft:temperate")));

        let mesh = mesh_entity(&pig_data(), &chunk);
        let (min, max) = mesh_bounds(&mesh);
        assert!((min[0] - 7.0625).abs() < 1e-4);
        assert!((max[0] - 8.5625).abs() < 1e-4);
        assert!((min[2] - 7.6875).abs() < 1e-4);
        assert!((max[2] - 8.3125).abs() < 1e-4);
    }

    #[test]
    fn cold_pigs_and_unsupported_entities_are_skipped() {
        let mut chunk = Chunk::new();
        assert!(chunk.set_entity(entity("minecraft:pig", 0.0, "minecraft:cold")));
        assert!(mesh_entity(&pig_data(), &chunk).is_empty());

        let mut chunk = Chunk::new();
        assert!(chunk.set_entity(entity("minecraft:cow", 0.0, "minecraft:warm")));
        assert!(mesh_entity(&cow_data(), &chunk).is_empty());

        let mut chunk = Chunk::new();
        assert!(chunk.set_entity(entity("minecraft:chicken", 0.0, "minecraft:cold")));
        assert!(mesh_entity(&chicken_data(), &chunk).is_empty());

        let mut chunk = Chunk::new();
        let root = Tag::Compound(vec![("id".into(), Tag::String("minecraft:sheep".into()))]);
        assert!(chunk.set_entity(mcre_world::WorldEntity::new(
            "minecraft:sheep".into(),
            [8.0, 64.0, 8.0],
            mcre_anvil::nbt::write_named("", &root),
        )));
        assert!(mesh_entity(&pig_data(), &chunk).is_empty());
    }

    #[test]
    fn cow_entity_bakes_model_into_chunk_mesh() {
        let mut chunk = Chunk::new();
        assert!(chunk.set_entity(entity("minecraft:cow", 0.0, "minecraft:temperate")));

        let mesh = mesh_entity(&cow_data(), &chunk);
        assert_eq!(mesh.opaque_meta.len(), 60);
        assert_eq!(mesh.indices.len(), 60 * 6);
        let (min, max) = mesh_bounds(&mesh);
        assert!((min[0] - 7.625).abs() < 1e-4);
        assert!((max[0] - 8.375).abs() < 1e-4);
        assert!((min[1] - 64.001).abs() < 1e-4);
        assert!((max[1] - 65.5635).abs() < 1e-4);
        assert!((min[2] - 7.375).abs() < 1e-4);
        assert!((max[2] - 8.9375).abs() < 1e-4);
    }

    #[test]
    fn chicken_entity_bakes_model_into_chunk_mesh() {
        let mut chunk = Chunk::new();
        assert!(chunk.set_entity(entity("minecraft:chicken", 0.0, "minecraft:temperate")));

        let mesh = mesh_entity(&chicken_data(), &chunk);
        assert_eq!(mesh.opaque_meta.len(), 48);
        assert_eq!(mesh.indices.len(), 48 * 6);
        let (min, max) = mesh_bounds(&mesh);
        assert!((min[0] - 7.75).abs() < 1e-4);
        assert!((max[0] - 8.25).abs() < 1e-4);
        assert!((min[1] - 64.001).abs() < 1e-4);
        assert!((max[1] - 64.9385).abs() < 1e-4);
        assert!((min[2] - 7.75).abs() < 1e-4);
        assert!((max[2] - 8.5).abs() < 1e-4);
    }

    #[test]
    fn creeper_entity_bakes_model_into_chunk_mesh() {
        let mut chunk = Chunk::new();
        assert!(chunk.set_entity(entity("minecraft:creeper", 0.0, "minecraft:temperate")));

        let mesh = mesh_entity(&creeper_data(), &chunk);
        assert_eq!(mesh.opaque_meta.len(), 36);
        assert_eq!(mesh.indices.len(), 36 * 6);
        let (min, max) = mesh_bounds(&mesh);
        assert!((min[0] - 7.75).abs() < 1e-4);
        assert!((max[0] - 8.25).abs() < 1e-4);
        assert!((min[1] - 64.001).abs() < 1e-4);
        assert!((max[1] - 65.626).abs() < 1e-4);
        assert!((min[2] - 7.625).abs() < 1e-4);
        assert!((max[2] - 8.375).abs() < 1e-4);
    }

    fn sheep_entity(color: Option<i8>, sheared: bool) -> mcre_world::WorldEntity {
        let mut fields = vec![
            ("id".into(), Tag::String("minecraft:sheep".into())),
            (
                "Rotation".into(),
                Tag::List(vec![Tag::Float(0.0), Tag::Float(0.0)]),
            ),
        ];
        if let Some(color) = color {
            fields.push(("Color".into(), Tag::Byte(color)));
        }
        if sheared {
            fields.push(("Sheared".into(), Tag::Byte(1)));
        }
        let root = Tag::Compound(fields);
        mcre_world::WorldEntity::new(
            "minecraft:sheep".into(),
            [8.0, 64.0, 8.0],
            mcre_anvil::nbt::write_named("", &root),
        )
    }

    #[test]
    fn sheep_entity_bakes_base_and_wool_layers() {
        let mut chunk = Chunk::new();
        assert!(chunk.set_entity(sheep_entity(None, false)));

        let mesh = mesh_entity_with_atlas(&sheep_data(), &sheep_atlas(), &chunk);
        assert_eq!(mesh.opaque_meta.len(), 72);
        assert_eq!(mesh.indices.len(), 72 * 6);
        let (min, max) = mesh_bounds(&mesh);
        assert!((min[0] - 7.640625).abs() < 1e-4);
        assert!((max[0] - 8.359375).abs() < 1e-4);
        assert!((min[1] - 64.001).abs() < 1e-4);
        assert!((max[1] - 65.4135).abs() < 1e-4);
        assert!((min[2] - 7.390625).abs() < 1e-4);
        assert!((max[2] - 8.875).abs() < 1e-4);
    }

    #[test]
    fn sheared_sheep_skips_wool_layer() {
        let mut chunk = Chunk::new();
        assert!(chunk.set_entity(sheep_entity(Some(14), true)));

        let mesh = mesh_entity_with_atlas(&sheep_data(), &sheep_atlas(), &chunk);
        assert_eq!(mesh.opaque_meta.len(), 36);
    }

    #[test]
    fn colored_sheep_tints_wool_layer() {
        let mut chunk = Chunk::new();
        assert!(chunk.set_entity(sheep_entity(Some(14), false)));

        let mesh = mesh_entity_with_atlas(&sheep_data(), &sheep_atlas(), &chunk);
        assert_eq!(mesh.opaque_meta.len(), 72);
        assert_eq!(mesh.vertices[0].color, [1.0, 1.0, 1.0]);
        let wool = mesh.vertices[36 * 4].color;
        assert!((wool[0] - 0.517647).abs() < 1e-4);
        assert!((wool[1] - 0.133333).abs() < 1e-4);
        assert!((wool[2] - 0.109804).abs() < 1e-4);
    }

    #[test]
    fn zombie_entity_bakes_model_into_chunk_mesh() {
        let mut chunk = Chunk::new();
        assert!(chunk.set_entity(entity("minecraft:zombie", 0.0, "minecraft:temperate")));

        let mesh = mesh_entity(&zombie_data(), &chunk);
        assert_eq!(mesh.opaque_meta.len(), 42);
        assert_eq!(mesh.indices.len(), 42 * 6);
        let (min, max) = mesh_bounds(&mesh);
        assert!((min[0] - 7.5).abs() < 1e-4);
        assert!((max[0] - 8.5).abs() < 1e-4);
        assert!((min[1] - 64.001).abs() < 1e-4);
        assert!((max[1] - 66.03225).abs() < 1e-4);
        assert!((min[2] - 7.71875).abs() < 1e-4);
        assert!((max[2] - 8.637211).abs() < 1e-4);
    }

    #[test]
    fn drowned_entity_bakes_zombie_model_into_chunk_mesh() {
        let mut chunk = Chunk::new();
        assert!(chunk.set_entity(entity("minecraft:drowned", 0.0, "minecraft:temperate")));

        let mesh = mesh_entity(&drowned_data(), &chunk);
        assert_eq!(mesh.opaque_meta.len(), 42);
        assert_eq!(mesh.indices.len(), 42 * 6);
        let (min, max) = mesh_bounds(&mesh);
        assert!((min[0] - 7.5).abs() < 1e-4);
        assert!((max[0] - 8.5).abs() < 1e-4);
        assert!((min[1] - 64.001).abs() < 1e-4);
        assert!((max[1] - 66.03225).abs() < 1e-4);
        assert!((min[2] - 7.71875).abs() < 1e-4);
        assert!((max[2] - 8.637211).abs() < 1e-4);
    }

    #[test]
    fn skeleton_entity_bakes_model_into_chunk_mesh() {
        let mut chunk = Chunk::new();
        assert!(chunk.set_entity(entity("minecraft:skeleton", 0.0, "minecraft:temperate")));

        let mesh = mesh_entity(&skeleton_data(), &chunk);
        assert_eq!(mesh.opaque_meta.len(), 42);
        assert_eq!(mesh.indices.len(), 42 * 6);
        let (min, max) = mesh_bounds(&mesh);
        assert!((min[0] - 7.625).abs() < 1e-4);
        assert!((max[0] - 8.375).abs() < 1e-4);
        assert!((min[1] - 64.001).abs() < 1e-4);
        assert!((max[1] - 66.03225).abs() < 1e-4);
        assert!((min[2] - 7.71875).abs() < 1e-4);
        assert!((max[2] - 8.28125).abs() < 1e-4);
    }

    #[test]
    fn wolf_entity_bakes_model_into_chunk_mesh() {
        let mut chunk = Chunk::new();
        assert!(chunk.set_entity(entity("minecraft:wolf", 0.0, "minecraft:pale")));

        let mesh = mesh_entity(&wolf_data(), &chunk);
        assert_eq!(mesh.opaque_meta.len(), 66);
        assert_eq!(mesh.indices.len(), 66 * 6);
        let (min, max) = mesh_bounds(&mesh);
        assert!((min[0] - 7.75).abs() < 1e-4);
        assert!((max[0] - 8.25).abs() < 1e-4);
        assert!((min[1] - 64.001).abs() < 1e-4);
        assert!((max[1] - 64.96975).abs() < 1e-4);
        assert!((min[2] - 7.155544).abs() < 1e-4);
        assert!((max[2] - 8.75).abs() < 1e-4);
    }

    #[test]
    fn wolf_variant_selects_texture() {
        let mut data = empty_data();
        data.texture_table = vec![
            "minecraft:entity/wolf/wolf".into(),
            "minecraft:entity/wolf/wolf_woods".into(),
        ];
        data.entity_textures.wolf_pale = 0;
        data.entity_textures.wolf_woods = 1;

        let mut chunk = Chunk::new();
        assert!(chunk.set_entity(entity("minecraft:wolf", 0.0, "minecraft:woods")));

        let mesh = mesh_entity_with_atlas(&data, &wolf_atlas(), &chunk);
        assert_eq!(mesh.opaque_meta.len(), 66);
        assert!(
            mesh.vertices
                .iter()
                .all(|vertex| vertex.uv[1] >= 0.5 - 1e-6)
        );
    }

    #[test]
    fn parrot_entity_bakes_model_into_chunk_mesh() {
        let mut chunk = Chunk::new();
        assert!(chunk.set_entity(parrot_entity(0)));

        let mesh = mesh_entity(&parrot_data(), &chunk);
        assert_eq!(mesh.opaque_meta.len(), 66);
        assert_eq!(mesh.indices.len(), 66 * 6);
        let (min, max) = mesh_bounds(&mesh);
        assert!((min[0] - 7.875).abs() < 1e-4);
        assert!((max[0] - 8.125).abs() < 1e-4);
        assert!((min[1] - 64.00012).abs() < 1e-4);
        assert!((max[1] - 64.9261).abs() < 1e-4);
        assert!((min[2] - 7.76822).abs() < 1e-4);
        assert!((max[2] - 8.44518).abs() < 1e-4);
    }

    #[test]
    fn parrot_variant_selects_texture() {
        let mut data = empty_data();
        data.texture_table = vec![
            "minecraft:entity/parrot/parrot_red_blue".into(),
            "minecraft:entity/parrot/parrot_yellow_blue".into(),
        ];
        data.entity_textures.parrot_red_blue = 0;
        data.entity_textures.parrot_yellow_blue = 1;

        let mut chunk = Chunk::new();
        assert!(chunk.set_entity(parrot_entity(3)));

        let mesh = mesh_entity_with_atlas(&data, &parrot_atlas(), &chunk);
        assert_eq!(mesh.opaque_meta.len(), 66);
        assert!(
            mesh.vertices
                .iter()
                .all(|vertex| vertex.uv[1] >= 0.5 - 1e-6)
        );
    }

    #[test]
    fn bat_entity_bakes_model_into_chunk_mesh() {
        let mut chunk = Chunk::new();
        assert!(chunk.set_entity(entity("minecraft:bat", 0.0, "minecraft:temperate")));

        let mesh = mesh_entity(&bat_data(), &chunk);
        assert_eq!(mesh.opaque_meta.len(), 54);
        assert_eq!(mesh.indices.len(), 54 * 6);
        let (min, max) = mesh_bounds(&mesh);
        assert!((min[0] - 7.40625).abs() < 1e-4);
        assert!((max[0] - 8.59375).abs() < 1e-4);
        assert!((min[1] - 64.001).abs() < 1e-4);
        assert!((max[1] - 64.8135).abs() < 1e-4);
        assert!((min[2] - 7.9375).abs() < 1e-4);
        assert!((max[2] - 8.0625).abs() < 1e-4);
    }

    #[test]
    fn bee_entity_bakes_model_into_chunk_mesh() {
        let mut chunk = Chunk::new();
        assert!(chunk.set_entity(entity("minecraft:bee", 0.0, "minecraft:temperate")));

        let mesh = mesh_entity(&bee_data(), &chunk);
        assert_eq!(mesh.opaque_meta.len(), 54);
        assert_eq!(mesh.indices.len(), 54 * 6);
        let (min, max) = mesh_bounds(&mesh);
        assert!((min[0] - 7.343688).abs() < 1e-4);
        assert!((max[0] - 8.656312).abs() < 1e-4);
        assert!((min[1] - 64.001).abs() < 1e-4);
        assert!((max[1] - 64.56356).abs() < 1e-4);
        assert!((min[2] - 7.5625).abs() < 1e-4);
        assert!((max[2] - 8.5).abs() < 1e-4);
    }

    #[test]
    fn enderman_entity_bakes_model_into_chunk_mesh() {
        let mut chunk = Chunk::new();
        assert!(chunk.set_entity(entity("minecraft:enderman", 0.0, "minecraft:temperate")));

        let mesh = mesh_entity(&enderman_data(), &chunk);
        assert_eq!(mesh.opaque_meta.len(), 36);
        assert_eq!(mesh.indices.len(), 36 * 6);
        let (min, max) = mesh_bounds(&mesh);
        assert!((min[0] - 7.625).abs() < 1e-4);
        assert!((max[0] - 8.375).abs() < 1e-4);
        assert!((min[1] - 63.9385).abs() < 1e-4);
        assert!((max[1] - 66.8135).abs() < 1e-4);
        assert!((min[2] - 7.75).abs() < 1e-4);
        assert!((max[2] - 8.25).abs() < 1e-4);
    }

    #[test]
    fn squid_entity_bakes_model_into_chunk_mesh() {
        let mut chunk = Chunk::new();
        assert!(chunk.set_entity(entity("minecraft:squid", 0.0, "minecraft:temperate")));

        let mesh = mesh_entity(&squid_data(), &chunk);
        assert_eq!(mesh.opaque_meta.len(), 54);
        assert_eq!(mesh.indices.len(), 54 * 6);
        let (min, max) = mesh_bounds(&mesh);
        assert!((min[0] - 7.62375).abs() < 1e-4);
        assert!((max[0] - 8.37625).abs() < 1e-4);
        assert!((min[1] - 63.4385).abs() < 1e-4);
        assert!((max[1] - 65.50225).abs() < 1e-4);
        assert!((min[2] - 7.62375).abs() < 1e-4);
        assert!((max[2] - 8.37625).abs() < 1e-4);
    }

    #[test]
    fn glow_squid_entity_uses_glow_texture() {
        let mut data = empty_data();
        data.texture_table = vec![
            "minecraft:entity/squid/squid".into(),
            "minecraft:entity/squid/glow_squid".into(),
        ];
        data.entity_textures.squid = 0;
        data.entity_textures.glow_squid = 1;

        let mut chunk = Chunk::new();
        assert!(chunk.set_entity(entity("minecraft:glow_squid", 0.0, "minecraft:temperate")));

        let mesh = mesh_entity_with_atlas(&data, &wolf_atlas(), &chunk);
        assert_eq!(mesh.opaque_meta.len(), 54);
        assert!(
            mesh.vertices
                .iter()
                .all(|vertex| vertex.uv[1] >= 0.5 - 1e-6)
        );
    }

    #[test]
    fn entity_part_vertex_applies_y_then_z_rotation() {
        let vertex = entity_part_vertex(
            [-15.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [
                0.0,
                std::f32::consts::FRAC_PI_4,
                -std::f32::consts::FRAC_PI_4,
            ],
        );
        assert!((vertex[0] - -7.5).abs() < 1e-4);
        assert!((vertex[1] - 7.5).abs() < 1e-4);
        assert!((vertex[2] - 10.6066).abs() < 1e-4);
    }

    #[test]
    fn model_box_growth_expands_geometry_only() {
        let plain = model_box_faces([0.0, 0.0, 0.0, 8.0, 8.0, 8.0], [0.0, 0.0], false, 0.0);
        let grown = model_box_faces([0.0, 0.0, 0.0, 8.0, 8.0, 8.0], [0.0, 0.0], false, 0.5);
        for (plain_face, grown_face) in plain.iter().zip(grown.iter()) {
            assert_eq!(plain_face.2, grown_face.2);
        }
        assert!((grown[2].1[0][0] + 0.5).abs() < 1e-6);
        assert!((grown[4].1[0][0] - 8.5).abs() < 1e-6);
        assert!((grown[0].1[0][1] + 0.5).abs() < 1e-6);
        assert!((grown[1].1[0][1] - 8.5).abs() < 1e-6);
    }

    #[test]
    fn baby_animals_are_skipped() {
        let root = Tag::Compound(vec![
            ("id".into(), Tag::String("minecraft:cow".into())),
            ("Age".into(), Tag::Int(-1)),
            ("variant".into(), Tag::String("minecraft:temperate".into())),
        ]);
        let mut chunk = Chunk::new();
        assert!(chunk.set_entity(mcre_world::WorldEntity::new(
            "minecraft:cow".into(),
            [8.0, 64.0, 8.0],
            mcre_anvil::nbt::write_named("", &root),
        )));
        assert!(mesh_entity(&cow_data(), &chunk).is_empty());
    }

    #[test]
    fn model_box_uvs_match_vanilla_layout() {
        let faces = model_box_faces([0.0, 0.0, 0.0, 8.0, 8.0, 8.0], [0.0, 0.0], false, 0.0);
        let east = faces
            .iter()
            .find(|(direction, _, _)| *direction == Direction::East)
            .unwrap();
        assert_eq!(
            east.2,
            [[24.0, 8.0], [16.0, 8.0], [16.0, 16.0], [24.0, 16.0]]
        );
        let south = faces
            .iter()
            .find(|(direction, _, _)| *direction == Direction::South)
            .unwrap();
        assert_eq!(
            south.2,
            [[32.0, 8.0], [24.0, 8.0], [24.0, 16.0], [32.0, 16.0]]
        );

        let mirrored = model_box_faces([0.0, 0.0, 0.0, 8.0, 8.0, 8.0], [0.0, 0.0], true, 0.0);
        let north = mirrored
            .iter()
            .find(|(direction, _, _)| *direction == Direction::North)
            .unwrap();
        assert_eq!(
            north.1,
            [
                [0.0, 8.0, 0.0],
                [8.0, 8.0, 0.0],
                [8.0, 0.0, 0.0],
                [0.0, 0.0, 0.0]
            ]
        );
        assert_eq!(
            north.2,
            [[16.0, 16.0], [8.0, 16.0], [8.0, 8.0], [16.0, 8.0]]
        );
    }

    #[test]
    fn water_surface_is_seamless_across_chunk_borders() {
        let data = empty_data();
        let atlas = test_atlas();
        let mut world = World::new(1);
        for (chunk_x, chunk_z) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            let mut chunk = Chunk::new_at(mcre_world::ChunkPos::new(chunk_x, chunk_z));
            for lx in 0..16 {
                for lz in 0..16 {
                    chunk.set_block(lx, 63, lz, Block::STONE.default_state());
                    chunk.set_block(lx, 64, lz, Block::WATER.default_state());
                }
            }
            world.replace_chunk(chunk);
        }

        let surface_y = 64.0 + 8.0 / 9.0 - 0.001;
        for (chunk_x, chunk_z, origin) in [
            (0, 0, [0, 0]),
            (1, 0, [16, 0]),
            (0, 1, [0, 16]),
            (1, 1, [16, 16]),
        ] {
            let pos = mcre_world::ChunkPos::new(chunk_x, chunk_z);
            let chunk = world.chunk_at_pos(pos).unwrap();
            let mesh = mesh_chunk(
                &data,
                &atlas,
                chunk,
                &world,
                origin,
                LightMode::Flat,
                MeshOptions::default(),
            );
            let mut border_vertices = 0;
            for quad in mesh.translucent_indices.chunks(6) {
                for &i in quad {
                    let p = mesh.vertices[i as usize].position;
                    let interior = p[2] > 2.0 && p[2] < 30.0;
                    if (p[0] - 16.0).abs() < 1e-4 && interior {
                        border_vertices += 1;
                        assert!(
                            (p[1] - surface_y).abs() < 1e-3,
                            "vertex at shared chunk border has y={} (expected {surface_y})",
                            p[1]
                        );
                    }
                }
            }
            assert!(
                border_vertices > 0,
                "no water quads reach the shared border in chunk ({chunk_x}, {chunk_z})"
            );
        }
    }

    #[test]
    fn class_specific_skip_rules() {
        let data = empty_data();

        // Vanilla default (`cutoutLeaves = true`) renders leaf-to-leaf faces.
        let oak_leaves = Block::OAK_LEAVES.default_state();
        let birch_leaves = Block::BIRCH_LEAVES.default_state();
        assert!(should_render_face(
            &data,
            oak_leaves,
            birch_leaves,
            Direction::Up,
            MeshOptions::default()
        ));
        assert!(should_render_face(
            &data,
            oak_leaves,
            air(),
            Direction::Up,
            MeshOptions::default()
        ));

        // Unconnected bars only cull vertical self faces; connected bars
        // cull along their connection axis.
        let bars = Block::IRON_BARS.default_state();
        assert!(should_render_face(
            &data,
            bars,
            bars,
            Direction::North,
            MeshOptions::default()
        ));
        assert!(!should_render_face(
            &data,
            bars,
            bars,
            Direction::Up,
            MeshOptions::default()
        ));
        let connected = (u16::from(Block::IRON_BARS.min_state())
            ..=u16::from(Block::IRON_BARS.max_state()))
            .map(BlockState::from)
            .find(|state| {
                state.is_north() && state.is_south() && !state.is_east() && !state.is_west()
            })
            .unwrap();
        assert!(!should_render_face(
            &data,
            connected,
            connected,
            Direction::North,
            MeshOptions::default()
        ));
        assert!(should_render_face(
            &data,
            connected,
            connected,
            Direction::East,
            MeshOptions::default()
        ));

        let stairs = Block::OAK_STAIRS.default_state();
        assert!(!should_render_face(
            &data,
            stairs,
            stairs,
            Direction::Up,
            MeshOptions::default()
        ));

        let mangrove = Block::MANGROVE_ROOTS.default_state();
        assert!(!should_render_face(
            &data,
            mangrove,
            mangrove,
            Direction::Up,
            MeshOptions::default()
        ));
        assert!(should_render_face(
            &data,
            mangrove,
            mangrove,
            Direction::North,
            MeshOptions::default()
        ));
    }

    #[test]
    fn cullface_quads_hidden_and_uncullface_quads_kept() {
        struct AllSolid;
        impl BlockAccess for AllSolid {
            fn block_at(&self, _x: i32, _y: i32, _z: i32) -> BlockState {
                Block::STONE.default_state()
            }
        }

        let data = stone_data(vec![quad(None), quad(Some(Direction::Up))]);
        let atlas = test_atlas();
        let mut chunk = Chunk::new();
        chunk.set_block(8, 0, 8, Block::STONE.default_state());

        let mesh = mesh_chunk(
            &data,
            &atlas,
            &chunk,
            &AllSolid,
            [0, 0],
            LightMode::Smooth,
            MeshOptions::default(),
        );
        assert_eq!(mesh.indices.len() / 6, 1);
    }

    #[test]
    fn unloaded_border_faces_are_kept() {
        let data = stone_data(vec![quad(Some(Direction::East))]);
        let atlas = test_atlas();
        let mut chunk = Chunk::new();
        chunk.set_block(15, 0, 8, Block::STONE.default_state());

        let world = World::new(0);
        let mesh = mesh_chunk(
            &data,
            &atlas,
            &chunk,
            &world,
            [0, 0],
            LightMode::Smooth,
            MeshOptions::default(),
        );
        assert_eq!(mesh.indices.len() / 6, 1);
    }

    fn mesh_two_quads(atlas: &Atlas) -> Mesh {
        let data = stone_data(vec![quad(Some(Direction::Up)), quad(None)]);
        let mut chunk = Chunk::new();
        chunk.set_block(8, 0, 8, Block::STONE.default_state());
        mesh_chunk(
            &data,
            atlas,
            &chunk,
            &World::new(0),
            [0, 0],
            LightMode::Smooth,
            MeshOptions::default(),
        )
    }

    #[test]
    fn quad_meta_records_facing_and_pass() {
        let mesh = mesh_two_quads(&test_atlas());
        assert_eq!(mesh.opaque_meta.len(), 2);
        assert_eq!(mesh.opaque_meta[0].facing, crate::vertex::FACING_POS_Y);
        assert_eq!(mesh.opaque_meta[0].pass, TerrainPass::Solid);
        assert_eq!(mesh.opaque_meta[0].block_pos, [8, 0, 8]);
        assert_eq!(
            u16::from(mesh.opaque_meta[0].state),
            u16::from(Block::STONE.default_state())
        );
        assert_eq!(mesh.opaque_meta[1].facing, crate::vertex::FACING_UNASSIGNED);
    }

    #[test]
    fn alpha_textures_classify_as_cutout() {
        let mut atlas = test_atlas();
        atlas.has_alpha = vec![true];
        let mesh = mesh_two_quads(&atlas);
        assert!(
            mesh.opaque_meta
                .iter()
                .all(|m| m.pass == TerrainPass::Cutout)
        );
    }

    #[test]
    fn pack_chunk_sections_assigns_by_block_y() {
        let data = stone_data(vec![quad(Some(Direction::Up))]);
        let atlas = test_atlas();
        let mut chunk = Chunk::new();
        chunk.set_block(8, 0, 8, Block::STONE.default_state());
        chunk.set_block(8, 17, 8, Block::STONE.default_state());
        let mesh = mesh_chunk(
            &data,
            &atlas,
            &chunk,
            &World::new(0),
            [0, 0],
            LightMode::Smooth,
            MeshOptions::default(),
        );

        let sections = pack_chunk_sections(&mesh, [0, 0]);
        let low = sections[4].as_ref().expect("section for y=0");
        assert_eq!(
            low.passes[0].counts[crate::vertex::FACING_POS_Y as usize],
            4
        );
        let decoded = crate::vertex::decode_position(low.passes[0].vertices[0].position);
        assert!((decoded[1] - 1.0).abs() < 1e-4, "local y: {decoded:?}");

        let high = sections[5].as_ref().expect("section for y=17");
        assert_eq!(
            high.passes[0].counts[crate::vertex::FACING_POS_Y as usize],
            4
        );
        let decoded = crate::vertex::decode_position(high.passes[0].vertices[0].position);
        assert!((decoded[1] - 2.0).abs() < 1e-4, "local y: {decoded:?}");

        assert!(sections[3].is_none());
        assert!(sections[6].is_none());
    }

    #[test]
    fn pack_layout_groups_facing_runs_contiguously() {
        let data = stone_data(vec![
            quad(Some(Direction::Up)),
            quad(Some(Direction::East)),
            quad(None),
        ]);
        let atlas = test_atlas();
        let mut chunk = Chunk::new();
        chunk.set_block(8, 0, 8, Block::STONE.default_state());
        let mesh = mesh_chunk(
            &data,
            &atlas,
            &chunk,
            &World::new(0),
            [0, 0],
            LightMode::Smooth,
            MeshOptions::default(),
        );

        let sections = pack_chunk_sections(&mesh, [0, 0]);
        let section = sections[4].as_ref().expect("section");
        let solid = &section.passes[0];
        let up = crate::vertex::FACING_POS_Y as usize;
        let east = crate::vertex::FACING_POS_X as usize;
        let unassigned = crate::vertex::FACING_UNASSIGNED as usize;
        assert_eq!(solid.counts[up], 4);
        assert_eq!(solid.counts[east], 4);
        assert_eq!(solid.counts[unassigned], 4);
        assert_eq!(solid.vertices.len(), 12);
        // Runs are laid out in facing-ordinal order: unassigned, +X, +Y.
        assert_eq!(solid.facing_offset(unassigned as u8), 0);
        assert_eq!(solid.facing_offset(east as u8), 4);
        assert_eq!(solid.facing_offset(up as u8), 8);
    }

    fn entry(tintindex: Option<u8>, shade: f32) -> mcre_resource_pack::lookup::BakedQuadEntry {
        mcre_resource_pack::lookup::BakedQuadEntry {
            vertices: [Vec3f::new(0.0, 0.0, 0.0); 4],
            uvs: [[0.0, 0.0]; 4],
            texture_id: 0,
            cullface: None,
            direction: Direction::Up,
            tintindex,
            shade,
            light_emission: 0,
            ambient_occlusion: true,
        }
    }

    struct SplitBiome;
    impl BlockAccess for SplitBiome {
        fn block_at(&self, _x: i32, _y: i32, _z: i32) -> BlockState {
            air()
        }

        fn biome_at(&self, x: i32, _y: i32, _z: i32) -> Biome {
            if x < 0 {
                Biome::BADLANDS
            } else {
                Biome::PALE_GARDEN
            }
        }
    }

    struct Badlands;
    impl BlockAccess for Badlands {
        fn block_at(&self, _x: i32, _y: i32, _z: i32) -> BlockState {
            air()
        }

        fn biome_at(&self, _x: i32, _y: i32, _z: i32) -> Biome {
            Biome::BADLANDS
        }
    }

    struct Swamp;
    impl BlockAccess for Swamp {
        fn block_at(&self, _x: i32, _y: i32, _z: i32) -> BlockState {
            air()
        }

        fn biome_at(&self, _x: i32, _y: i32, _z: i32) -> Biome {
            Biome::SWAMP
        }
    }

    struct YSplitBiome;
    impl BlockAccess for YSplitBiome {
        fn block_at(&self, _x: i32, _y: i32, _z: i32) -> BlockState {
            air()
        }

        fn biome_at(&self, _x: i32, y: i32, _z: i32) -> Biome {
            if y < 10 {
                Biome::BADLANDS
            } else {
                Biome::PLAINS
            }
        }
    }

    fn tint_of(
        data: &ResourcePackData,
        access: &impl BlockAccess,
        state: BlockState,
        tintindex: Option<u8>,
    ) -> [f32; 3] {
        quad_tint(data, access, state, [0, 10, 0], &entry(tintindex, 1.0))
    }

    #[test]
    fn quad_color_multiplies_shade_and_tint() {
        let data = empty_data();
        let access = SplitBiome;
        let stone = Block::STONE.default_state();
        assert_eq!(
            quad_color(&data, &access, stone, [0, 10, 0], &entry(None, 0.5)),
            [0.5, 0.5, 0.5]
        );
        assert_eq!(
            quad_color(&data, &access, stone, [0, 10, 0], &entry(Some(9), 0.6)),
            [0.6, 0.6, 0.6]
        );
    }

    #[test]
    fn grass_override_tint_beats_colormap() {
        let data = empty_data();
        let access = Badlands;
        let grass = Block::GRASS_BLOCK.default_state();
        assert_eq!(
            tint_of(&data, &access, grass, Some(0)),
            [144.0 / 255.0, 129.0 / 255.0, 77.0 / 255.0]
        );
    }

    #[test]
    fn swamp_grass_resolve_matches_jvm_threshold() {
        use mcre_core::noise::{SWAMP_DRY, SWAMP_WET};
        let data = empty_data();
        // (100, 200) is wet and (42, 42) dry per the live-JVM oracle.
        assert_eq!(
            biome_grass_color(Biome::SWAMP, 100, 200, &data.colormaps),
            SWAMP_WET,
        );
        assert_eq!(
            biome_grass_color(Biome::SWAMP, 42, 42, &data.colormaps),
            SWAMP_DRY,
        );
    }

    #[test]
    fn swamp_tint_blends_jvm_oracle_window() {
        // Live-JVM noise for the 25 columns around the origin: the x=-2
        // column is wet (< -0.1), the other 20 are dry. Expected tint is
        // the integer-division average both implementations compute.
        let expected = [100.0 / 255.0, 113.0 / 255.0, 57.0 / 255.0];
        let data = empty_data();
        let access = Swamp;
        let grass = Block::GRASS_BLOCK.default_state();
        assert_eq!(tint_of(&data, &access, grass, Some(0)), expected);
    }

    #[test]
    fn biome_blend_passes_sample_positions() {
        let data = empty_data();
        let access = Swamp;
        let seen = std::cell::RefCell::new(Vec::new());
        average_biome_color(&access, [10, 64, 20], &data.colormaps, |_, x, z, _| {
            seen.borrow_mut().push((x, z));
            [0, 0, 0]
        });
        let mut expected = Vec::new();
        for dx in -2..=2 {
            for dz in -2..=2 {
                expected.push((10 + dx, 20 + dz));
            }
        }
        let seen = seen.into_inner();
        assert_eq!(seen, expected);
    }

    #[test]
    fn biome_blend_averages_5x5_cells() {
        let data = empty_data();
        let access = SplitBiome;
        let grass = Block::GRASS_BLOCK.default_state();
        assert_eq!(
            tint_of(&data, &access, grass, Some(0)),
            [129.0 / 255.0, 129.0 / 255.0, 99.0 / 255.0]
        );
    }

    #[test]
    fn static_tint_sources() {
        let data = empty_data();
        let access = SplitBiome;
        let spruce = Block::SPRUCE_LEAVES.default_state();
        assert_eq!(
            tint_of(&data, &access, spruce, Some(0)),
            [
                0x61 as f32 / 255.0,
                0x99 as f32 / 255.0,
                0x61 as f32 / 255.0
            ]
        );
        assert_eq!(
            particle_tint(&data, &access, spruce, [0, 10, 0]),
            tint_of(&data, &access, spruce, Some(0))
        );
        let vine = Block::VINE.default_state();
        assert_eq!(
            particle_tint(&data, &access, vine, [0, 10, 0]),
            tint_of(&data, &access, vine, Some(0))
        );
        let petals = Block::PINK_PETALS.default_state();
        assert_eq!(tint_of(&data, &access, petals, Some(0)), [1.0, 1.0, 1.0]);
        let redstone = Block::REDSTONE_WIRE.default_state();
        assert_eq!(
            tint_of(&data, &access, redstone, Some(0)),
            [76.0 / 255.0, 0.0, 0.0]
        );
        let stem = Block::MELON_STEM.default_state();
        assert_eq!(tint_of(&data, &access, stem, Some(0)), [0.0, 1.0, 0.0]);
    }

    #[test]
    fn tall_grass_upper_samples_below() {
        let data = empty_data();
        let access = YSplitBiome;
        let upper = (u16::from(Block::TALL_GRASS.min_state())
            ..=u16::from(Block::TALL_GRASS.max_state()))
            .map(BlockState::from)
            .find(|state| state.doubleblock_half() == DoubleBlockHalf::Upper)
            .unwrap();
        assert_eq!(
            tint_of(&data, &access, upper, Some(0)),
            [144.0 / 255.0, 129.0 / 255.0, 77.0 / 255.0]
        );
    }

    #[test]
    fn atlas_uv_stays_inside_region() {
        let atlas = Atlas {
            width: 256,
            height: 256,
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
        };
        let region = AtlasRegion {
            x: 16,
            y: 32,
            width: 16,
            height: 16,
        };
        // Edge UVs land exactly on the region bounds (anti-bleed comes from
        // the centroid-bias encoding, not an inset).
        let uv = atlas_uv(region, &atlas, &[0.0, 0.0]);
        assert_eq!(uv, [16.0 / 256.0, 32.0 / 256.0]);

        let uv = atlas_uv(region, &atlas, &[16.0, 16.0]);
        assert_eq!(uv, [32.0 / 256.0, 48.0 / 256.0]);

        let mid = atlas_uv(region, &atlas, &[8.0, 8.0]);
        assert!(mid[0] > 16.0 / 256.0 && mid[0] < 32.0 / 256.0);
        assert!(mid[1] > 32.0 / 256.0 && mid[1] < 48.0 / 256.0);
    }

    /// Open sky (15, 15) with a solid wall plane at `x <= 7`, so the west
    /// edge and west diagonals of a face at `x = 8` are occluded.
    struct BrightWall;
    impl BlockAccess for BrightWall {
        fn block_at(&self, x: i32, _y: i32, _z: i32) -> BlockState {
            if x <= 7 {
                Block::STONE.default_state()
            } else {
                air()
            }
        }

        fn light_at(&self, _x: i32, _y: i32, _z: i32) -> (u8, u8) {
            (15, 15)
        }
    }

    fn wall_chunk() -> Chunk {
        let mut chunk = Chunk::new();
        chunk.set_block(8, 0, 8, Block::STONE.default_state());
        chunk
    }

    fn assert_color(vertex: &Vertex, expected: f32) {
        for channel in vertex.color {
            assert!((channel - expected).abs() < 1e-6, "{channel} != {expected}");
        }
    }

    #[test]
    fn smooth_ao_darkens_edges_against_wall() {
        let data = stone_data(vec![quad(Some(Direction::Up))]);
        let atlas = test_atlas();

        let mesh = mesh_chunk(
            &data,
            &atlas,
            &wall_chunk(),
            &BrightWall,
            [0, 0],
            LightMode::Smooth,
            MeshOptions::default(),
        );
        assert_eq!(mesh.vertices.len(), 4);

        // Pairings + AmbientVertexRemap(Up): computed vertices 2 and 3 pair
        // the occluded west edge and land in slots 0 and 1.
        assert_color(&mesh.vertices[0], 0.6);
        assert_color(&mesh.vertices[1], 0.6);
        assert_color(&mesh.vertices[2], 1.0);
        assert_color(&mesh.vertices[3], 1.0);
        for vertex in &mesh.vertices {
            assert_eq!(vertex.light, [15.0, 15.0]);
        }
    }

    #[test]
    fn corner_edges_maps_positions_to_edge_cells() {
        let corners = adjacency_corners(Direction::Up);
        assert_eq!(
            corner_edges(Direction::Up, &Vec3f::new(0.0, 16.0, 0.0), &corners),
            Some((1, 2))
        );
        assert_eq!(
            corner_edges(Direction::Up, &Vec3f::new(16.0, 16.0, 16.0), &corners),
            Some((0, 3))
        );
        assert_eq!(
            corner_edges(Direction::Up, &Vec3f::new(8.0, 16.0, 0.0), &corners),
            None
        );

        let corners = adjacency_corners(Direction::North);
        assert_eq!(
            corner_edges(Direction::North, &Vec3f::new(16.0, 16.0, 0.0), &corners),
            Some((2, 0))
        );
        assert_eq!(
            corner_edges(Direction::North, &Vec3f::new(16.0, 8.0, 0.0), &corners),
            None
        );
    }

    #[test]
    fn smooth_blend_keeps_quarter_levels() {
        let blended = smooth_blend((15, 15), (15, 15), (15, 15), (14, 15));
        assert_eq!(blended, (14.75, 15.0));
        let blended = smooth_blend((15, 15), (15, 15), (15, 15), (15, 15));
        assert_eq!(blended, (15.0, 15.0));
    }

    #[test]
    fn noncubic_quads_weight_by_coverage() {
        let stone = Block::STONE.default_state();
        let east_half = [
            Vec3f::new(8.0, 16.0, 0.0),
            Vec3f::new(8.0, 16.0, 16.0),
            Vec3f::new(16.0, 16.0, 16.0),
            Vec3f::new(16.0, 16.0, 0.0),
        ];
        let (colors, lights) = smooth_face(
            &BrightWall,
            Direction::Up,
            [8, 0, 8],
            &east_half,
            [1.0; 3],
            stone,
        );
        for c in &colors {
            for v in c {
                assert!(v.is_finite() && (0.0..=1.0).contains(v), "{v}");
            }
        }
        for l in &lights {
            for v in l {
                assert!(v.is_finite() && (0.0..=15.0).contains(v), "{v}");
            }
        }
        assert!(colors[0][0] < colors[2][0], "west edge should be darker");
        assert!(colors[1][0] < colors[3][0], "west edge should be darker");

        let tie = [
            Vec3f::new(16.0, 16.0, 0.0),
            Vec3f::new(16.0, 0.0, 0.0),
            Vec3f::new(8.0, 0.0, 0.0),
            Vec3f::new(8.0, 16.0, 0.0),
        ];
        let (colors, _) = smooth_face(
            &BrightWall,
            Direction::North,
            [8, 0, 8],
            &tie,
            [1.0; 3],
            stone,
        );
        for c in colors.iter().flatten() {
            assert!(c.is_finite() && (0.0..=1.0).contains(c), "{c}");
        }
    }

    #[test]
    fn smooth_ao_attachment_is_variant_stable() {
        // Same Up face in authored order vs y-90-permuted order: AO must
        // attach to world corners identically, so only UVs rotate with the
        // variant. Rotated slot (i + 1) % 4 sits on authored slot i's corner.
        let authored = [
            Vec3f::new(0.0, 16.0, 0.0),
            Vec3f::new(0.0, 16.0, 16.0),
            Vec3f::new(16.0, 16.0, 16.0),
            Vec3f::new(16.0, 16.0, 0.0),
        ];
        let rotated = [
            Vec3f::new(16.0, 16.0, 0.0),
            Vec3f::new(0.0, 16.0, 0.0),
            Vec3f::new(0.0, 16.0, 16.0),
            Vec3f::new(16.0, 16.0, 16.0),
        ];
        let stone = Block::STONE.default_state();
        let (colors_a, _) = smooth_face(
            &BrightWall,
            Direction::Up,
            [8, 0, 8],
            &authored,
            [1.0; 3],
            stone,
        );
        let (colors_b, _) = smooth_face(
            &BrightWall,
            Direction::Up,
            [8, 0, 8],
            &rotated,
            [1.0; 3],
            stone,
        );
        for i in 0..4 {
            assert_eq!(colors_a[i], colors_b[(i + 1) % 4], "corner {i} moved");
        }
        // AO is active, not flat: the west edge stays darkened in both.
        assert!((colors_a[0][0] - 0.6).abs() < 1e-6);
        assert_eq!(colors_a[2], [1.0, 1.0, 1.0]);
    }

    struct FullBlockEast;

    impl BlockAccess for FullBlockEast {
        fn block_at(&self, x: i32, y: i32, z: i32) -> BlockState {
            if (x, y, z) == (9, 0, 8) {
                Block::STONE.default_state()
            } else {
                air()
            }
        }

        fn light_at(&self, _x: i32, _y: i32, _z: i32) -> (u8, u8) {
            (15, 15)
        }
    }

    #[test]
    fn partial_face_ao_samples_owner_cell() {
        // Bottom slab top face: vanilla treats this as non-cubic, so the
        // corner cells circle the slab's own cell; the full block east at
        // the same level reaches the east corners.
        let top = [
            Vec3f::new(0.0, 8.0, 0.0),
            Vec3f::new(0.0, 8.0, 16.0),
            Vec3f::new(16.0, 8.0, 16.0),
            Vec3f::new(16.0, 8.0, 0.0),
        ];
        let (colors, _) = smooth_face(
            &FullBlockEast,
            Direction::Up,
            [8, 0, 8],
            &top,
            [1.0; 3],
            Block::STONE_SLAB.default_state(),
        );
        assert_eq!(colors[0], [1.0, 1.0, 1.0]);
        assert_eq!(colors[1], [1.0, 1.0, 1.0]);
        assert!((colors[2][0] - 0.8).abs() < 1e-6, "{}", colors[2][0]);
        assert!((colors[3][0] - 0.8).abs() < 1e-6, "{}", colors[3][0]);
    }

    struct PillarOverhang;

    impl BlockAccess for PillarOverhang {
        fn block_at(&self, x: i32, y: i32, z: i32) -> BlockState {
            match (x, y, z) {
                (9, 1, 8) | (9, 2, 8) | (8, 2, 7) => Block::STONE.default_state(),
                _ => air(),
            }
        }

        fn light_at(&self, _x: i32, _y: i32, _z: i32) -> (u8, u8) {
            (15, 15)
        }
    }

    #[test]
    fn diagonal_reuse_gate_samples_above_edge_cell() {
        // Two-tall pillar east plus a floating block above the north edge
        // cell: the near diagonal reuses the pillar's corner shade, the far
        // diagonal reads the open cell.
        let top = [
            Vec3f::new(0.0, 16.0, 0.0),
            Vec3f::new(0.0, 16.0, 16.0),
            Vec3f::new(16.0, 16.0, 16.0),
            Vec3f::new(16.0, 16.0, 0.0),
        ];
        let (colors, _) = smooth_face(
            &PillarOverhang,
            Direction::Up,
            [8, 0, 8],
            &top,
            [1.0; 3],
            Block::STONE.default_state(),
        );
        assert_eq!(colors[0], [1.0, 1.0, 1.0]);
        assert_eq!(colors[1], [1.0, 1.0, 1.0]);
        assert!((colors[2][0] - 0.8).abs() < 1e-6, "{}", colors[2][0]);
        assert!((colors[3][0] - 0.6).abs() < 1e-6, "{}", colors[3][0]);
    }

    #[test]
    fn shade_brightness_overrides_are_extracted() {
        assert_eq!(shade_of(Block::STONE.default_state()), 0.2);
        // TransparentBlock keeps soft shading despite a full collision shape.
        assert_eq!(shade_of(Block::GLASS.default_state()), 1.0);
        let layers = |state: BlockState| state.layers();
        let snow_eight = (u16::from(Block::SNOW.min_state())..=u16::from(Block::SNOW.max_state()))
            .map(BlockState::from)
            .find(|state| layers(*state) == 8)
            .unwrap();
        assert_eq!(shade_of(snow_eight), 0.2);
        let snow_one = (u16::from(Block::SNOW.min_state())..=u16::from(Block::SNOW.max_state()))
            .map(BlockState::from)
            .find(|state| layers(*state) == 1)
            .unwrap();
        assert_eq!(shade_of(snow_one), 1.0);
    }

    struct OwnerLightOnly;

    impl BlockAccess for OwnerLightOnly {
        fn block_at(&self, _x: i32, _y: i32, _z: i32) -> BlockState {
            air()
        }

        fn light_at(&self, x: i32, y: i32, z: i32) -> (u8, u8) {
            if (x, y, z) == (8, 0, 8) {
                (14, 15)
            } else {
                (0, 15)
            }
        }
    }

    #[test]
    fn flat_partial_quads_sample_owner_cell_light() {
        let slab = Block::STONE_SLAB.default_state();
        let top = [
            Vec3f::new(0.0, 8.0, 0.0),
            Vec3f::new(0.0, 8.0, 16.0),
            Vec3f::new(16.0, 8.0, 16.0),
            Vec3f::new(16.0, 8.0, 0.0),
        ];
        let (_, lights) = flat_face(
            &OwnerLightOnly,
            Direction::Up,
            [8, 0, 8],
            &top,
            false,
            slab,
            [1.0; 3],
        );
        assert_eq!(lights, [[14.0, 15.0]; 4]);
        let (_, lights) = flat_face(
            &OwnerLightOnly,
            Direction::Up,
            [8, 0, 8],
            &top,
            true,
            slab,
            [1.0; 3],
        );
        assert_eq!(lights, [[0.0, 15.0]; 4]);
    }

    #[test]
    fn flat_mode_uses_one_light_sample_and_no_ao() {
        let data = stone_data(vec![quad(Some(Direction::Up))]);
        let atlas = test_atlas();

        let mesh = mesh_chunk(
            &data,
            &atlas,
            &wall_chunk(),
            &BrightWall,
            [0, 0],
            LightMode::Flat,
            MeshOptions::default(),
        );
        assert_eq!(mesh.vertices.len(), 4);
        for vertex in &mesh.vertices {
            assert_color(vertex, 1.0);
            assert_eq!(vertex.light, [15.0, 15.0]);
        }
    }

    #[test]
    fn emissive_and_disabled_ao_quads_light_flat() {
        let atlas = test_atlas();

        let mut emissive = quad(Some(Direction::Up));
        emissive.light_emission = 15;
        let data = stone_data(vec![emissive]);
        let mesh = mesh_chunk(
            &data,
            &atlas,
            &wall_chunk(),
            &BrightWall,
            [0, 0],
            LightMode::Smooth,
            MeshOptions::default(),
        );
        for vertex in &mesh.vertices {
            assert_color(vertex, 1.0);
            assert_eq!(vertex.light, [15.0, 15.0]);
        }

        let mut no_ao = quad(Some(Direction::Up));
        no_ao.ambient_occlusion = false;
        let data = stone_data(vec![no_ao]);
        let mesh = mesh_chunk(
            &data,
            &atlas,
            &wall_chunk(),
            &BrightWall,
            [0, 0],
            LightMode::Smooth,
            MeshOptions::default(),
        );
        for vertex in &mesh.vertices {
            assert_color(vertex, 1.0);
            assert_eq!(vertex.light, [15.0, 15.0]);
        }
    }

    #[test]
    fn emissive_states_light_flat_fullbright() {
        struct DimMagma;
        impl BlockAccess for DimMagma {
            fn block_at(&self, x: i32, y: i32, z: i32) -> BlockState {
                if (x, y, z) == (8, 0, 8) {
                    Block::MAGMA_BLOCK.default_state()
                } else {
                    air()
                }
            }

            fn light_at(&self, _x: i32, _y: i32, _z: i32) -> (u8, u8) {
                (0, 0)
            }
        }

        let data = state_data(
            Block::MAGMA_BLOCK.default_state(),
            vec![quad(Some(Direction::Up))],
            flat_colormaps(),
        );
        let atlas = test_atlas();
        let mut chunk = Chunk::new();
        chunk.set_block(8, 0, 8, Block::MAGMA_BLOCK.default_state());
        let mesh = mesh_chunk(
            &data,
            &atlas,
            &chunk,
            &DimMagma,
            [0, 0],
            LightMode::Smooth,
            MeshOptions::default(),
        );
        assert_eq!(mesh.vertices.len(), 4);
        for vertex in &mesh.vertices {
            assert_color(vertex, 1.0);
            assert_eq!(vertex.light, [15.0, 15.0]);
        }
    }

    #[test]
    fn flat_light_channels_carry_raw_levels() {
        struct Dim;
        impl BlockAccess for Dim {
            fn block_at(&self, _x: i32, _y: i32, _z: i32) -> BlockState {
                air()
            }

            fn light_at(&self, _x: i32, _y: i32, _z: i32) -> (u8, u8) {
                (8, 4)
            }
        }

        let data = stone_data(vec![quad(None)]);
        let atlas = test_atlas();
        let mut chunk = Chunk::new();
        chunk.set_block(0, 0, 0, Block::STONE.default_state());

        let mesh = mesh_chunk(
            &data,
            &atlas,
            &chunk,
            &Dim,
            [0, 0],
            LightMode::Flat,
            MeshOptions::default(),
        );
        assert_eq!(mesh.vertices.len(), 4);
        for vertex in &mesh.vertices {
            assert_eq!(vertex.light, [8.0, 4.0]);
        }
    }

    #[test]
    fn legacy_random_matches_java_util_random() {
        // Oracle values from `java.util.Random` (the same LCG as vanilla's
        // `SingleThreadedRandomSource`) and the `Mth.getSeed` formula.
        let seeds: [([i32; 3], i64); 8] = [
            ([0, 0, 0], 0),
            ([1, 0, 0], 133076631897947),
            ([0, 1, 0], 645),
            ([0, 0, 1], -20769809646864),
            ([-1, -5, 3], -32314142176065),
            ([8, 0, 8], -90067312761588),
            ([15, 100, -7], -94090614483273),
            ([123, -45, 678], -49831574764955),
        ];
        for (pos, seed) in seeds {
            assert_eq!(position_seed(pos[0], pos[1], pos[2]), seed, "seed {pos:?}");
        }

        let draws: [(i64, i32, i32); 35] = [
            (0, 1, 0),
            (0, 2, 1),
            (0, 3, 0),
            (0, 4, 2),
            (0, 5, 0),
            (0, 6, 0),
            (0, 16, 11),
            (133076631897947, 2, 0),
            (133076631897947, 3, 1),
            (133076631897947, 5, 3),
            (133076631897947, 6, 1),
            (133076631897947, 16, 1),
            (-20769809646864, 2, 0),
            (-20769809646864, 3, 2),
            (-20769809646864, 4, 1),
            (-20769809646864, 5, 1),
            (-20769809646864, 6, 5),
            (-20769809646864, 16, 7),
            (-32314142176065, 3, 2),
            (-32314142176065, 5, 2),
            (-32314142176065, 16, 0),
            (-94090614483273, 4, 3),
            (-94090614483273, 6, 4),
            (-94090614483273, 16, 13),
            (-49831574764955, 6, 2),
            (-49831574764955, 16, 9),
            (-4962768465676381896, 2, 0),
            (-4962768465676381896, 3, 0),
            (-4962768465676381896, 6, 0),
            (2257715297775777127, 2, 0),
            (2257715297775777127, 3, 1),
            (2257715297775777127, 6, 1),
            (-5605584720434349240, 2, 1),
            (-5605584720434349240, 3, 0),
            (-5605584720434349240, 6, 0),
        ];
        for (seed, bound, expected) in draws {
            let mut rng = LegacyRandom::new(seed);
            assert_eq!(
                rng.next_int(bound),
                expected,
                "next_int({bound}) at seed {seed}"
            );
        }

        let longs: [(i64, i64); 8] = [
            (0, -4962768465676381896),
            (133076631897947, 2257715297775777127),
            (645, -5605584720434349240),
            (-20769809646864, 8198620391012991421),
            (-32314142176065, 953947514584650543),
            (-90067312761588, 3559662578969015940),
            (-94090614483273, -2530915703121154626),
            (-49831574764955, -7046880197858726353),
        ];
        for (seed, expected) in longs {
            let mut rng = LegacyRandom::new(seed);
            assert_eq!(rng.next_long(), expected, "next_long at seed {seed}");
        }
    }

    fn variant_entry(weights: &[u8]) -> mcre_resource_pack::lookup::BlockStateEntry {
        use mcre_resource_pack::lookup::{BlockRule, BlockStateEntry, BlockVariant};
        let variants = weights
            .iter()
            .enumerate()
            .map(|(i, &weight)| {
                let mut q = quad(None);
                q.texture_id = i as u32;
                BlockVariant {
                    weight,
                    quads: vec![q],
                }
            })
            .collect();
        BlockStateEntry {
            rules: vec![BlockRule { variants }],
        }
    }

    fn picked_textures(
        entry: &mcre_resource_pack::lookup::BlockStateEntry,
        pos: [i32; 3],
    ) -> Vec<u32> {
        let mut picked = Vec::new();
        for_each_variant(entry, pos, |variant| {
            picked.push(variant.quads[0].texture_id);
        });
        picked
    }

    #[test]
    fn weighted_variants_selected_by_position_seed() {
        // Two variants, weight 1 each (total 2, power-of-two branch).
        let entry = variant_entry(&[1, 1]);
        assert_eq!(picked_textures(&entry, [0, 0, 0]), [1]);
        assert_eq!(picked_textures(&entry, [1, 0, 0]), [0]);
        assert_eq!(picked_textures(&entry, [0, 1, 0]), [1]);
        assert_eq!(picked_textures(&entry, [0, 0, 1]), [0]);

        // Weights 1/2/3 (total 6, rejection-sampling branch).
        let entry = variant_entry(&[1, 2, 3]);
        assert_eq!(picked_textures(&entry, [0, 0, 0]), [0]);
        assert_eq!(picked_textures(&entry, [1, 0, 0]), [1]);
        assert_eq!(picked_textures(&entry, [0, 1, 0]), [0]);
        assert_eq!(picked_textures(&entry, [0, 0, 1]), [2]);
        assert_eq!(picked_textures(&entry, [-1, -5, 3]), [2]);
        assert_eq!(picked_textures(&entry, [8, 0, 8]), [0]);
        assert_eq!(picked_textures(&entry, [15, 100, -7]), [2]);
        assert_eq!(picked_textures(&entry, [123, -45, 678]), [1]);
    }

    #[test]
    fn multipart_rules_share_one_reseeded_sequence() {
        use mcre_resource_pack::lookup::{BlockRule, BlockStateEntry, BlockVariant};
        let mut single = quad(None);
        single.texture_id = 100;
        let entry = BlockStateEntry {
            rules: vec![
                BlockRule {
                    variants: vec![BlockVariant {
                        weight: 1,
                        quads: vec![single],
                    }],
                },
                BlockRule {
                    variants: (200..202)
                        .map(|texture_id| {
                            let mut q = quad(None);
                            q.texture_id = texture_id;
                            BlockVariant {
                                weight: 1,
                                quads: vec![q],
                            }
                        })
                        .collect(),
                },
            ],
        };
        // Seed 0 draws nextLong -4962768465676381896, re-seeded per part;
        // the weighted part then draws nextInt(2) = 0.
        assert_eq!(picked_textures(&entry, [0, 0, 0]), [100, 200]);
        // Same position re-meshes identically.
        assert_eq!(picked_textures(&entry, [0, 0, 0]), [100, 200]);
    }

    #[test]
    fn variant_pick_changes_uvs_but_not_ao() {
        // One rule with four single-quad variants differing only in UVs.
        // World (8,0,8) picks variant 0 and (8,1,8) picks variant 3
        // (java.util.Random oracle); both sit against the west BrightWall
        // with congruent neighborhoods.
        let quads = (0..4)
            .map(|i| {
                let mut q = quad(Some(Direction::Up));
                q.uvs = [[i as f32 * 4.0, 0.0]; 4];
                q
            })
            .collect::<Vec<_>>();
        let atlas = test_atlas();
        let data = stone_data_multi(quads);

        let mesh_at = |y: i32| {
            let mut chunk = Chunk::new();
            chunk.set_block(8, y, 8, Block::STONE.default_state());
            mesh_chunk(
                &data,
                &atlas,
                &chunk,
                &BrightWall,
                [0, 0],
                LightMode::Smooth,
                MeshOptions::default(),
            )
        };
        let mesh0 = mesh_at(0);
        let mesh1 = mesh_at(1);
        assert_eq!(mesh0.vertices.len(), 4);
        assert_eq!(mesh1.vertices.len(), 4);

        // Different variants: UVs differ.
        assert_ne!(mesh0.vertices[0].uv, mesh1.vertices[0].uv);
        // Same symmetric top quad under congruent neighborhoods: AO identical.
        for (a, b) in mesh0.vertices.iter().zip(mesh1.vertices.iter()) {
            assert_eq!(a.color, b.color);
            assert_eq!(a.light, b.light);
        }
        // And the west edge is actually darkened (AO is active, not flat).
        assert_color(&mesh0.vertices[0], 0.6);
        assert_color(&mesh0.vertices[3], 1.0);
    }
}
