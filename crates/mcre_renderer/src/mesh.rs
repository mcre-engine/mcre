use bytemuck::{Pod, Zeroable};
use mcre_core::{Axis, Direction, GrassColorModifier};
use mcre_resource_pack::{
    Colormaps, ResourcePackData, TintSource,
    colormap::{DRY_FOLIAGE_DEFAULT, FOLIAGE_DEFAULT, GRASS_DEFAULT, sample_colormap},
    lookup::{BlockRule, BlockVariant},
    tint_sources,
};
use mcre_world::shape::{face_boxes, is_covered};
use mcre_world::{Biome, Block, BlockState, Chunk, DoubleBlockHalf, World};

use crate::atlas::{Atlas, AtlasRegion};

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

#[derive(Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    /// Opaque quads (block models and lava fluids).
    pub indices: Vec<u32>,
    /// Translucent quads (water fluids), drawn in a blended pass after
    /// `indices`.
    pub translucent_indices: Vec<u32>,
}

impl Mesh {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty() && self.translucent_indices.is_empty()
    }
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
                    flat_face(access, quad.direction, block_pos, state, base_color)
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

                mesh.indices.extend_from_slice(&[
                    base,
                    base + 1,
                    base + 2,
                    base,
                    base + 2,
                    base + 3,
                ]);
                if options.double_sided_liquids && state.block().liquid_fluid().is_some() {
                    mesh.indices.extend_from_slice(&[
                        base + 2,
                        base + 1,
                        base,
                        base + 3,
                        base + 2,
                        base,
                    ]);
                }
            }
        });
    }

    mesh
}

/// Vanilla `Block.getShadeBrightness`: collision-full cells darken
/// neighboring corner samples to 0.2
/// (`state.isCollisionShapeFullBlock(EmptyBlockGetter.INSTANCE, BlockPos.ZERO)`).
fn shade_of(state: BlockState) -> f32 {
    if state.collision_full() { 0.2 } else { 1.0 }
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
fn smooth_blend(n1: (u8, u8), n2: (u8, u8), n3: (u8, u8), center: (u8, u8)) -> (u8, u8) {
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
    let block = (u32::from(neighbors[0].0)
        + u32::from(neighbors[1].0)
        + u32::from(neighbors[2].0)
        + u32::from(center.0))
        >> 2;
    let sky = (u32::from(neighbors[0].1)
        + u32::from(neighbors[1].1)
        + u32::from(neighbors[2].1)
        + u32::from(center.1))
        >> 2;
    (block as u8, sky as u8)
}

/// Vanilla `prepareQuadFlat`: one light sample on the outside of the face,
/// raised to the quad owner's emission; no corner AO.
fn flat_face<A: BlockAccess + ?Sized>(
    access: &A,
    direction: Direction,
    block_pos: [i32; 3],
    owner: BlockState,
    color: [f32; 3],
) -> ([[f32; 3]; 4], [[f32; 2]; 4]) {
    let base = offset(block_pos, direction);
    let (block, sky) = access.light_at(base[0], base[1], base[2]);
    let (block, sky) = if owner.emissive_rendering() {
        (15, 15)
    } else {
        (block.max(owner.light_emission()), sky)
    };
    let light = [f32::from(block), f32::from(sky)];
    ([color; 4], [light; 4])
}

/// Vanilla `prepareQuadAmbientOcclusion`, cubic-face branch: per-vertex blend
/// of two edge cells, their diagonal, and the face-center cell, with the
/// four shade samples averaged into an AO multiplier.
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
    let base = offset(block_pos, direction);
    let corners = adjacency_corners(direction);

    let mut edge_light = [(0_u8, 0_u8); 4];
    let mut edge_shade = [1.0_f32; 4];
    let mut edge_permeable = [false; 4];
    for (index, &corner) in corners.iter().enumerate() {
        let pos = offset(base, corner);
        edge_light[index] = sample_light(access, pos);
        let state = access.block_at(pos[0], pos[1], pos[2]);
        edge_shade[index] = shade_of(state);
        edge_permeable[index] = is_light_permeable(state);
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

    let bounds = face_bounds(verts);
    let partial = face_partial(direction, &bounds);
    let cubic = face_cubic(direction, &bounds, owner.collision_full());
    let neighbor = access.block_at(base[0], base[1], base[2]);
    let (center_light, center_shade) = if cubic || !neighbor.solid_render() {
        (sample_light(access, base), shade_of(neighbor))
    } else {
        (sample_light(access, block_pos), shade_of(owner))
    };

    let pairings = [(3, 0), (2, 0), (2, 1), (3, 1)];
    let mut tblend = [(0_u8, 0_u8); 4];
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
            lights[i] = [f32::from(blended.0), f32::from(blended.1)];
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
        lights[i] = [f32::from(blended.0), f32::from(blended.1)];
        colors[i] = [color[0] * ambient, color[1] * ambient, color[2] * ambient];
    }
    (colors, lights)
}

fn smooth_weighted_blend(lights: [(u8, u8); 4], weights: [f32; 4]) -> (u8, u8) {
    let block = (f32::from(lights[0].0) * weights[0]
        + f32::from(lights[1].0) * weights[1]
        + f32::from(lights[2].0) * weights[2]
        + f32::from(lights[3].0) * weights[3]) as u8;
    let sky = (f32::from(lights[0].1) * weights[0]
        + f32::from(lights[1].1) * weights[1]
        + f32::from(lights[2].1) * weights[2]
        + f32::from(lights[3].1) * weights[3]) as u8;
    (block.min(15), sky.min(15))
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

/// Vanilla `Block.shouldRenderFace` at full-cube fidelity: hide the face when
/// the neighbor's occlusion shape is a full cube, or when the extracted
/// `skipRendering` data says the pair culls. Partial-vs-partial shape
/// intersection is deferred to the voxel-shape tier.
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
    let occluder = face_boxes(neighbor.occlusion_boxes(), direction.opposite());
    if occluder.is_empty() {
        return true;
    }
    let shape = face_boxes(state.occlusion_boxes(), direction);
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
/// edge-to-edge with zero gutter, so the mapping is inset by a sub-texel
/// epsilon: edge UVs land just inside the region instead of on texel centers
/// of the first/last texel row.
pub(crate) fn atlas_uv(region: AtlasRegion, atlas: &Atlas, uv: &[f32; 2]) -> [f32; 2] {
    let u = (uv[0] / 16.0).clamp(0.0, 1.0);
    let v = (uv[1] / 16.0).clamp(0.0, 1.0);
    let eps = 0.01;
    let x0 = region.x as f32 + eps;
    let x1 = (region.x + region.width) as f32 - eps;
    let y0 = region.y as f32 + eps;
    let y1 = (region.y + region.height) as f32 - eps;
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
    rgb_to_float(match source {
        TintSource::Blank => [255, 255, 255],
        TintSource::Constant(rgb) => *rgb,
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
    })
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
    use mcre_world::{Half, SlabType, StairsShape};

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
            entries,
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
            entries: vec![],
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
            entries,
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
            entries,
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
            regions: vec![],
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
            regions: vec![],
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
        // Edge UVs land a sub-texel epsilon inside the region instead of on
        // the centers of the first/last texel row.
        let uv = atlas_uv(region, &atlas, &[0.0, 0.0]);
        assert_eq!(uv, [(16.0 + 0.01) / 256.0, (32.0 + 0.01) / 256.0]);

        let uv = atlas_uv(region, &atlas, &[16.0, 16.0]);
        let x0 = 16.0 + 0.01;
        let x1 = (16.0 + 16.0) - 0.01;
        let y0 = 32.0 + 0.01;
        let y1 = (32.0 + 16.0) - 0.01;
        assert_eq!(
            uv,
            [
                (x0 + 1.0 * (x1 - x0)) / 256.0,
                (y0 + 1.0 * (y1 - y0)) / 256.0
            ]
        );

        let mid = atlas_uv(region, &atlas, &[8.0, 8.0]);
        assert!(mid[0] > (16.0 + 0.01) / 256.0 && mid[0] < (32.0 - 0.01) / 256.0);
        assert!(mid[1] > (32.0 + 0.01) / 256.0 && mid[1] < (48.0 - 0.01) / 256.0);
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
