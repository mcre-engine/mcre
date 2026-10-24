//! Port of vanilla `FluidRenderer.tesselate` (26.4): fluid boxes built from
//! neighbor heights with flow-aligned UVs, water overlay side textures and
//! vanilla face occlusion/back-face rules.

use mcre_core::{Axis, Direction};
use mcre_resource_pack::ResourcePackData;
use mcre_world::BlockState;
use mcre_world::fluid::{FluidKind, FluidState, MAX_FLUID_HEIGHT, fluid_state, is_face_sturdy};
use mcre_world::shape::fluid_face_occluded;

use crate::atlas::{Atlas, AtlasRegion};
use crate::mesh::{
    BlockAccess, Mesh, MeshOptions, QuadMeta, Vertex, atlas_uv, sample_light, water_tint,
};
use crate::vertex::{FACING_NEG_Y, FACING_POS_Y, TerrainPass, facing_of, opposite_facing};

/// Vanilla `FluidRenderer` face offset toward the rendered face.
const FACE_OFFSET: f32 = 0.001;
/// `CardinalLighting.DEFAULT` factors used for fluid face colors.
const SHADE_DOWN: f32 = 0.5;
const SHADE_SIDE_Z: f32 = 0.8;
const SHADE_SIDE_X: f32 = 0.6;

const HORIZONTALS: [Direction; 4] = [
    Direction::North,
    Direction::South,
    Direction::West,
    Direction::East,
];

fn offset(pos: [i32; 3], direction: Direction) -> [i32; 3] {
    let delta = direction.delta();
    [pos[0] + delta[0], pos[1] + delta[1], pos[2] + delta[2]]
}

fn block_at<A: BlockAccess + ?Sized>(access: &A, pos: [i32; 3]) -> BlockState {
    access.block_at(pos[0], pos[1], pos[2])
}

fn light_at<A: BlockAccess + ?Sized>(access: &A, pos: [i32; 3]) -> [f32; 2] {
    let (block, sky) = sample_light(access, pos);
    [f32::from(block), f32::from(sky)]
}

fn max_light(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0].max(b[0]), a[1].max(b[1])]
}

fn same_fluid(kind: FluidKind, fluid: Option<FluidState>) -> bool {
    fluid.is_some_and(|fluid| fluid.kind == kind)
}

fn affects_flow(kind: FluidKind, fluid: Option<FluidState>) -> bool {
    fluid.is_none() || same_fluid(kind, fluid)
}

/// Vanilla `FluidRenderer.isFaceOccludedBySelf`.
fn is_face_occluded_by_self(state: BlockState, direction: Direction) -> bool {
    fluid_face_occluded(direction.opposite(), 1.0, state.occlusion_boxes())
}

/// Vanilla `FluidRenderer.isFaceOccludedByNeighbor`.
fn is_face_occluded_by_neighbor(direction: Direction, height: f32, state: BlockState) -> bool {
    fluid_face_occluded(direction, height, state.occlusion_boxes())
}

/// Vanilla `FluidRenderer.shouldRenderFace`.
fn should_render_face(
    fluid: FluidState,
    state: BlockState,
    direction: Direction,
    neighbor: Option<FluidState>,
) -> bool {
    !same_fluid(fluid.kind, neighbor) && !is_face_occluded_by_self(state, direction)
}

/// Vanilla `FlowingFluid.isSolidFace` (infinite empty getter).
fn is_solid_face<A: BlockAccess + ?Sized>(
    access: &A,
    pos: [i32; 3],
    direction: Direction,
    kind: FluidKind,
) -> bool {
    let state = block_at(access, pos);
    if same_fluid(kind, fluid_state(state)) {
        return false;
    }
    if direction == Direction::Up {
        return true;
    }
    if state.block().is_ice() {
        return false;
    }
    is_face_sturdy(state, direction)
}

fn normalize2(v: [f32; 2]) -> [f32; 2] {
    let len = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if len < 1.0e-4 {
        [0.0, 0.0]
    } else {
        [v[0] / len, v[1] / len]
    }
}

/// Vanilla `FlowingFluid.getFlow`.
fn get_flow<A: BlockAccess + ?Sized>(
    access: &A,
    kind: FluidKind,
    pos: [i32; 3],
    fluid: FluidState,
) -> [f32; 3] {
    let own_height = fluid.own_height();
    let mut flow = [0.0_f32; 2];
    for direction in HORIZONTALS {
        let neighbor_pos = offset(pos, direction);
        let neighbor_fluid = fluid_state(block_at(access, neighbor_pos));
        if !affects_flow(kind, neighbor_fluid) {
            continue;
        }
        let neighbor_height = neighbor_fluid.map_or(0.0, |fluid| fluid.own_height());
        let mut distance = 0.0;
        if neighbor_height == 0.0 {
            if !block_at(access, neighbor_pos).block().blocks_fluid_flow() {
                let below_pos = offset(neighbor_pos, Direction::Down);
                let below_fluid = fluid_state(block_at(access, below_pos));
                if affects_flow(kind, below_fluid) {
                    let below_height = below_fluid.map_or(0.0, |fluid| fluid.own_height());
                    if below_height > 0.0 {
                        distance = own_height - (below_height - MAX_FLUID_HEIGHT);
                    }
                }
            }
        } else if neighbor_height > 0.0 {
            distance = own_height - neighbor_height;
        }
        if distance != 0.0 {
            let delta = direction.delta();
            flow[0] += delta[0] as f32 * distance;
            flow[1] += delta[2] as f32 * distance;
        }
    }

    if fluid.falling {
        for direction in HORIZONTALS {
            let neighbor_pos = offset(pos, direction);
            let above_neighbor = offset(neighbor_pos, Direction::Up);
            if is_solid_face(access, neighbor_pos, direction, kind)
                || is_solid_face(access, above_neighbor, direction, kind)
            {
                let flat = normalize2(flow);
                let v = [flat[0], -6.0, flat[1]];
                let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                return [v[0] / len, v[1] / len, v[2] / len];
            }
        }
    }

    let flat = normalize2(flow);
    [flat[0], 0.0, flat[1]]
}

/// Vanilla `FluidRenderer.getHeight(level, fluidType, pos, state, fluidState)`.
fn get_height(
    kind: FluidKind,
    state: BlockState,
    fluid: Option<FluidState>,
    above_fluid: Option<FluidState>,
) -> f32 {
    if same_fluid(kind, fluid) {
        if same_fluid(kind, above_fluid) {
            1.0
        } else {
            fluid.map_or(0.0, |fluid| fluid.own_height())
        }
    } else if !state.solid() {
        0.0
    } else {
        -1.0
    }
}

fn get_height_at<A: BlockAccess + ?Sized>(access: &A, kind: FluidKind, pos: [i32; 3]) -> f32 {
    let state = block_at(access, pos);
    let above = block_at(access, offset(pos, Direction::Up));
    get_height(kind, state, fluid_state(state), fluid_state(above))
}

/// Vanilla `FluidRenderer.addWeightedHeight`.
fn add_weighted_height(weighted: &mut [f32; 2], height: f32) {
    if height >= 0.8 {
        weighted[0] += height * 10.0;
        weighted[1] += 10.0;
    } else if height >= 0.0 {
        weighted[0] += height;
        weighted[1] += 1.0;
    }
}

/// Vanilla `FluidRenderer.calculateAverageHeight`.
fn calculate_average_height<A: BlockAccess + ?Sized>(
    access: &A,
    kind: FluidKind,
    height_self: f32,
    height2: f32,
    height1: f32,
    corner_pos: [i32; 3],
) -> f32 {
    if height1 >= 1.0 || height2 >= 1.0 {
        return 1.0;
    }
    let mut weighted = [0.0_f32; 2];
    if height1 > 0.0 || height2 > 0.0 {
        let height_corner = get_height_at(access, kind, corner_pos);
        if height_corner >= 1.0 {
            return 1.0;
        }
        add_weighted_height(&mut weighted, height_corner);
    }
    add_weighted_height(&mut weighted, height_self);
    add_weighted_height(&mut weighted, height1);
    add_weighted_height(&mut weighted, height2);
    weighted[0] / weighted[1]
}

/// Vanilla `FluidState.shouldRenderBackwardUpFace`.
fn should_render_backward_up_face<A: BlockAccess + ?Sized>(
    access: &A,
    kind: FluidKind,
    above: [i32; 3],
) -> bool {
    for dx in -1..=1 {
        for dz in -1..=1 {
            let pos = [above[0] + dx, above[1], above[2] + dz];
            let state = block_at(access, pos);
            if !same_fluid(kind, fluid_state(state)) && !state.solid_render() {
                return true;
            }
        }
    }
    false
}

fn sprite_u(region: AtlasRegion, atlas: &Atlas, u: f32) -> f32 {
    atlas_uv(region, atlas, &[u * 16.0, 0.0])[0]
}

fn sprite_v(region: AtlasRegion, atlas: &Atlas, v: f32) -> f32 {
    atlas_uv(region, atlas, &[0.0, v * 16.0])[1]
}

fn scaled(tint: [f32; 3], factor: f32) -> [f32; 3] {
    [tint[0] * factor, tint[1] * factor, tint[2] * factor]
}

fn push_face(
    mesh: &mut Mesh,
    vertices: [Vertex; 4],
    block_pos: [i32; 3],
    state: BlockState,
    facing: u8,
    translucent: bool,
    back_face: bool,
) {
    let base = mesh.vertices.len() as u32;
    mesh.vertices.extend_from_slice(&vertices);
    let pass = if translucent {
        TerrainPass::Translucent
    } else {
        TerrainPass::Cutout
    };
    let (indices, metas) = if translucent {
        (&mut mesh.translucent_indices, &mut mesh.translucent_meta)
    } else {
        (&mut mesh.indices, &mut mesh.opaque_meta)
    };
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    metas.push(QuadMeta {
        facing,
        pass,
        block_pos,
        state,
    });
    if back_face {
        indices.extend_from_slice(&[base + 2, base + 1, base, base + 3, base + 2, base]);
        metas.push(QuadMeta {
            facing: opposite_facing(facing),
            pass,
            block_pos,
            state,
        });
    }
}

/// Vanilla `FluidRenderer.tesselate` for one fluid cell.
#[allow(clippy::too_many_arguments)]
pub(crate) fn mesh_fluid<A: BlockAccess + ?Sized>(
    mesh: &mut Mesh,
    data: &ResourcePackData,
    atlas: &Atlas,
    access: &A,
    pos: [i32; 3],
    state: BlockState,
    fluid: FluidState,
    options: MeshOptions,
) {
    let mut states = [state; 6];
    let mut fluids = [None; 6];
    for direction in [
        Direction::Down,
        Direction::Up,
        Direction::North,
        Direction::South,
        Direction::West,
        Direction::East,
    ] {
        let neighbor_pos = offset(pos, direction);
        let neighbor_state = block_at(access, neighbor_pos);
        states[direction.index() as usize] = neighbor_state;
        fluids[direction.index() as usize] = fluid_state(neighbor_state);
    }
    let neighbor = |direction: Direction| {
        (
            states[direction.index() as usize],
            fluids[direction.index() as usize],
        )
    };

    let (above_state, above_fluid) = neighbor(Direction::Up);
    let (below_state, below_fluid) = neighbor(Direction::Down);
    let (_, north_fluid) = neighbor(Direction::North);
    let (_, south_fluid) = neighbor(Direction::South);
    let (_, west_fluid) = neighbor(Direction::West);
    let (_, east_fluid) = neighbor(Direction::East);

    let render_up = !same_fluid(fluid.kind, above_fluid);
    let render_down = should_render_face(fluid, state, Direction::Down, below_fluid)
        && !is_face_occluded_by_neighbor(Direction::Down, MAX_FLUID_HEIGHT, below_state);
    let render_north = should_render_face(fluid, state, Direction::North, north_fluid);
    let render_south = should_render_face(fluid, state, Direction::South, south_fluid);
    let render_west = should_render_face(fluid, state, Direction::West, west_fluid);
    let render_east = should_render_face(fluid, state, Direction::East, east_fluid);
    if !(render_up || render_down || render_north || render_south || render_west || render_east) {
        return;
    }

    let translucent = fluid.kind == FluidKind::Water;
    let tint = if translucent {
        water_tint(data, access, pos)
    } else {
        [1.0; 3]
    };
    let (still_id, flow_id, overlay_id) = match fluid.kind {
        FluidKind::Water => (
            data.fluid_textures.water_still,
            data.fluid_textures.water_flow,
            Some(data.fluid_textures.water_overlay),
        ),
        FluidKind::Lava => (
            data.fluid_textures.lava_still,
            data.fluid_textures.lava_flow,
            None,
        ),
    };
    let still = atlas.region(still_id);
    let flow_region = atlas.region(flow_id);
    let overlay_region = overlay_id.map(|id| atlas.region(id));

    let height_self = get_height_at(access, fluid.kind, pos);
    let (height_ne, height_nw, height_se, height_sw) = if height_self >= 1.0 {
        (1.0, 1.0, 1.0, 1.0)
    } else {
        let height_north = get_height_at(access, fluid.kind, offset(pos, Direction::North));
        let height_south = get_height_at(access, fluid.kind, offset(pos, Direction::South));
        let height_east = get_height_at(access, fluid.kind, offset(pos, Direction::East));
        let height_west = get_height_at(access, fluid.kind, offset(pos, Direction::West));
        (
            calculate_average_height(
                access,
                fluid.kind,
                height_self,
                height_north,
                height_east,
                offset(offset(pos, Direction::North), Direction::East),
            ),
            calculate_average_height(
                access,
                fluid.kind,
                height_self,
                height_north,
                height_west,
                offset(offset(pos, Direction::North), Direction::West),
            ),
            calculate_average_height(
                access,
                fluid.kind,
                height_self,
                height_south,
                height_east,
                offset(offset(pos, Direction::South), Direction::East),
            ),
            calculate_average_height(
                access,
                fluid.kind,
                height_self,
                height_south,
                height_west,
                offset(offset(pos, Direction::South), Direction::West),
            ),
        )
    };

    let x = pos[0] as f32;
    let y = pos[1] as f32;
    let z = pos[2] as f32;
    let bottom_offs = if render_down { FACE_OFFSET } else { 0.0 };
    let self_light = light_at(access, pos);
    let self_above_light = max_light(self_light, light_at(access, offset(pos, Direction::Up)));

    if render_up {
        let min_corner = height_nw.min(height_sw).min(height_se).min(height_ne);
        if !is_face_occluded_by_neighbor(Direction::Up, min_corner, above_state) {
            let (nw, sw, se, ne) = (
                height_nw - FACE_OFFSET,
                height_sw - FACE_OFFSET,
                height_se - FACE_OFFSET,
                height_ne - FACE_OFFSET,
            );
            let flow = get_flow(access, fluid.kind, pos, fluid);
            let uvs = if flow[0] == 0.0 && flow[2] == 0.0 {
                let (u0, v0) = (sprite_u(still, atlas, 0.0), sprite_v(still, atlas, 0.0));
                let (u1, v1) = (sprite_u(still, atlas, 1.0), sprite_v(still, atlas, 1.0));
                [[u0, v0], [u0, v1], [u1, v1], [u1, v0]]
            } else {
                let angle = flow[2].atan2(flow[0]) - core::f32::consts::FRAC_PI_2;
                let s = angle.sin() * 0.25;
                let c = angle.cos() * 0.25;
                [
                    [
                        sprite_u(flow_region, atlas, 0.5 + (-c - s)),
                        sprite_v(flow_region, atlas, 0.5 + (-c + s)),
                    ],
                    [
                        sprite_u(flow_region, atlas, 0.5 + (-c + s)),
                        sprite_v(flow_region, atlas, 0.5 + (c + s)),
                    ],
                    [
                        sprite_u(flow_region, atlas, 0.5 + (c + s)),
                        sprite_v(flow_region, atlas, 0.5 + (c - s)),
                    ],
                    [
                        sprite_u(flow_region, atlas, 0.5 + (c - s)),
                        sprite_v(flow_region, atlas, 0.5 + (-c - s)),
                    ],
                ]
            };
            let back_face = options.double_sided_liquids
                && should_render_backward_up_face(access, fluid.kind, offset(pos, Direction::Up));
            let color = scaled(tint, 1.0);
            let light = self_above_light;
            push_face(
                mesh,
                [
                    Vertex {
                        position: [x, y + nw, z],
                        uv: uvs[0],
                        color,
                        light,
                    },
                    Vertex {
                        position: [x, y + sw, z + 1.0],
                        uv: uvs[1],
                        color,
                        light,
                    },
                    Vertex {
                        position: [x + 1.0, y + se, z + 1.0],
                        uv: uvs[2],
                        color,
                        light,
                    },
                    Vertex {
                        position: [x + 1.0, y + ne, z],
                        uv: uvs[3],
                        color,
                        light,
                    },
                ],
                pos,
                state,
                FACING_POS_Y,
                translucent,
                back_face,
            );
        }
    }

    if render_down {
        let (u0, v0) = (sprite_u(still, atlas, 0.0), sprite_v(still, atlas, 0.0));
        let (u1, v1) = (sprite_u(still, atlas, 1.0), sprite_v(still, atlas, 1.0));
        let light = max_light(light_at(access, offset(pos, Direction::Down)), self_light);
        let color = scaled(tint, SHADE_DOWN);
        push_face(
            mesh,
            [
                Vertex {
                    position: [x, y + bottom_offs, z],
                    uv: [u0, v0],
                    color,
                    light,
                },
                Vertex {
                    position: [x + 1.0, y + bottom_offs, z],
                    uv: [u1, v0],
                    color,
                    light,
                },
                Vertex {
                    position: [x + 1.0, y + bottom_offs, z + 1.0],
                    uv: [u1, v1],
                    color,
                    light,
                },
                Vertex {
                    position: [x, y + bottom_offs, z + 1.0],
                    uv: [u0, v1],
                    color,
                    light,
                },
            ],
            pos,
            state,
            FACING_NEG_Y,
            translucent,
            false,
        );
    }

    for face_dir in HORIZONTALS {
        let (hh0, hh1, x0, z0, x1, z1, render, face_state) = match face_dir {
            Direction::North => (
                height_nw,
                height_ne,
                x,
                z + FACE_OFFSET,
                x + 1.0,
                z + FACE_OFFSET,
                render_north,
                neighbor(Direction::North).0,
            ),
            Direction::South => (
                height_se,
                height_sw,
                x + 1.0,
                z + 1.0 - FACE_OFFSET,
                x,
                z + 1.0 - FACE_OFFSET,
                render_south,
                neighbor(Direction::South).0,
            ),
            Direction::West => (
                height_sw,
                height_nw,
                x + FACE_OFFSET,
                z + 1.0,
                x + FACE_OFFSET,
                z,
                render_west,
                neighbor(Direction::West).0,
            ),
            Direction::East => (
                height_ne,
                height_se,
                x + 1.0 - FACE_OFFSET,
                z,
                x + 1.0 - FACE_OFFSET,
                z + 1.0,
                render_east,
                neighbor(Direction::East).0,
            ),
            _ => unreachable!(),
        };
        if !render || is_face_occluded_by_neighbor(face_dir, hh0.max(hh1), face_state) {
            continue;
        }

        let is_overlay = overlay_region.is_some()
            && (face_state.block().half_transparent()
                || face_state.block().culling_tags().contains(&"leaves"));
        let region = match (is_overlay, overlay_region) {
            (true, Some(overlay)) => overlay,
            _ => flow_region,
        };
        let u0 = sprite_u(region, atlas, 0.0);
        let u1 = sprite_u(region, atlas, 0.5);
        let v01 = sprite_v(region, atlas, (1.0 - hh0) * 0.5);
        let v02 = sprite_v(region, atlas, (1.0 - hh1) * 0.5);
        let v1 = sprite_v(region, atlas, 0.5);
        let shade = if face_dir.axis() == Axis::Z {
            SHADE_SIDE_Z
        } else {
            SHADE_SIDE_X
        };
        let color = scaled(tint, shade);
        let light = self_above_light;
        push_face(
            mesh,
            [
                Vertex {
                    position: [x0, y + hh0, z0],
                    uv: [u0, v01],
                    color,
                    light,
                },
                Vertex {
                    position: [x1, y + hh1, z1],
                    uv: [u1, v02],
                    color,
                    light,
                },
                Vertex {
                    position: [x1, y + bottom_offs, z1],
                    uv: [u1, v1],
                    color,
                    light,
                },
                Vertex {
                    position: [x0, y + bottom_offs, z0],
                    uv: [u0, v1],
                    color,
                    light,
                },
            ],
            pos,
            state,
            facing_of(face_dir),
            translucent,
            options.double_sided_liquids && !is_overlay,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcre_resource_pack::colormap::Colormaps;
    use mcre_world::{Block, SlabType};

    struct Cells<F>(F);

    impl<F: Fn(i32, i32, i32) -> BlockState> BlockAccess for Cells<F> {
        fn block_at(&self, x: i32, y: i32, z: i32) -> BlockState {
            (self.0)(x, y, z)
        }
    }

    fn air() -> BlockState {
        Block::AIR.default_state()
    }

    fn water(level: u8) -> BlockState {
        (u16::from(Block::WATER.min_state())..=u16::from(Block::WATER.max_state()))
            .map(BlockState::from)
            .find(|state| state.level() == level)
            .expect("water level state")
    }

    fn data() -> ResourcePackData {
        ResourcePackData {
            fluid_textures: Default::default(),
            entries: vec![],
            particle_texture_ids: vec![],
            texture_table: vec![],
            culling: Default::default(),
            colormaps: Colormaps {
                grass: Box::new([]),
                foliage: Box::new([]),
                dry_foliage: Box::new([]),
            },
        }
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

    fn mesh_of(
        pos: [i32; 3],
        state: BlockState,
        access: &impl BlockAccess,
        options: MeshOptions,
    ) -> Mesh {
        let fluid = fluid_state(state).expect("fluid state");
        let mut mesh = Mesh::default();
        mesh_fluid(
            &mut mesh,
            &data(),
            &atlas(),
            access,
            pos,
            state,
            fluid,
            options,
        );
        mesh
    }

    #[test]
    fn isolated_source_renders_box_with_vanilla_back_faces() {
        let pos = [5, 60, 7];
        let access = Cells(|x, y, z| if [x, y, z] == pos { water(0) } else { air() });
        let mesh = mesh_of(pos, water(0), &access, MeshOptions::default());

        assert!(mesh.indices.is_empty());
        // Top, bottom, and four side faces; back faces add indices only.
        assert_eq!(mesh.vertices.len(), 6 * 4);
        assert_eq!(mesh.translucent_indices.len(), 11 * 6);
    }

    #[test]
    fn pack_puts_back_faces_in_opposite_facing_with_flipped_normal() {
        let pos = [5, 60, 7];
        let access = Cells(|x, y, z| if [x, y, z] == pos { water(0) } else { air() });
        let mesh = mesh_of(pos, water(0), &access, MeshOptions::default());

        let sections = crate::mesh::pack_chunk_sections(&mesh, [0, 0]);
        let section_y = ((pos[1] - mcre_world::WORLD_MIN_Y) / 16) as usize;
        let section = sections[section_y].as_ref().expect("fluid section");
        let translucent = &section.passes[2];

        let up = crate::vertex::FACING_POS_Y as usize;
        let down = crate::vertex::FACING_NEG_Y as usize;
        let pos_x = crate::vertex::FACING_POS_X as usize;
        let neg_x = crate::vertex::FACING_NEG_X as usize;
        let pos_z = crate::vertex::FACING_POS_Z as usize;
        let neg_z = crate::vertex::FACING_NEG_Z as usize;
        // Fronts and backs pair up on each axis except down (vanilla emits no
        // bottom back face): top front (+Y), top back + down front (−Y), and
        // each side with its reversed copy.
        assert_eq!(translucent.counts[up], 4);
        assert_eq!(translucent.counts[down], 8);
        assert_eq!(translucent.counts[pos_x], 8);
        assert_eq!(translucent.counts[neg_x], 8);
        assert_eq!(translucent.counts[pos_z], 8);
        assert_eq!(translucent.counts[neg_z], 8);
        assert_eq!(translucent.vertices.len(), 11 * 4);

        let normal_y = |vertex: &crate::vertex::TerrainVertex| (vertex.normal_tangent >> 8) & 0xFF;
        let up_first = &translucent.vertices[translucent.facing_offset(up as u8) as usize];
        let down_first = &translucent.vertices[translucent.facing_offset(down as u8) as usize];
        assert_eq!(normal_y(up_first), 127, "up front face normal +Y");
        assert_eq!(normal_y(down_first), 0, "top back face normal -Y");
    }

    #[test]
    fn same_fluid_above_removes_top_face() {
        let pos = [0, 62, 0];
        let access = Cells(|x, y, z| match (x, y, z) {
            (0, 62, 0) => water(0),
            (0, 63, 0) => water(8),
            _ => air(),
        });
        let mesh = mesh_of(pos, water(0), &access, MeshOptions::default());

        // Bottom plus four sides at full height (back faces add indices).
        assert_eq!(mesh.vertices.len(), 5 * 4);
        assert_eq!(mesh.translucent_indices.len(), 9 * 6);
    }

    #[test]
    fn single_sided_liquids_drop_back_faces() {
        let pos = [1, 61, 1];
        let access = Cells(|x, y, z| if [x, y, z] == pos { water(0) } else { air() });
        let options = MeshOptions {
            double_sided_liquids: false,
            ..MeshOptions::default()
        };
        let mesh = mesh_of(pos, water(0), &access, options);

        assert_eq!(mesh.vertices.len(), 6 * 4);
        assert_eq!(mesh.translucent_indices.len(), 6 * 6);
    }

    #[test]
    fn full_block_below_suppresses_bottom_face() {
        let pos = [2, 59, 2];
        let stone = Block::STONE.default_state();
        let access = Cells(|x, y, z| match (x, y, z) {
            (2, 59, 2) => water(0),
            (2, 58, 2) => stone,
            _ => air(),
        });
        let mesh = mesh_of(pos, water(0), &access, MeshOptions::default());

        // Top (front + back) and four sides (front + back), no bottom.
        assert_eq!(mesh.vertices.len(), 5 * 4);
        assert_eq!(mesh.translucent_indices.len(), 10 * 6);
    }

    #[test]
    fn full_block_side_neighbor_culls_side_face() {
        let pos = [4, 40, 4];
        let stone = Block::STONE.default_state();
        let access = Cells(|x, y, z| match (x, y, z) {
            (4, 40, 4) => water(0),
            (5, 40, 4) => stone,
            _ => air(),
        });
        let mesh = mesh_of(pos, water(0), &access, MeshOptions::default());

        // Top, bottom, north, south, west; east culled by the full block.
        assert_eq!(mesh.vertices.len(), 5 * 4);
        assert_eq!(mesh.translucent_indices.len(), 9 * 6);
    }

    #[test]
    fn glass_side_neighbor_keeps_side_face_with_overlay() {
        let pos = [6, 41, 6];
        let glass = Block::GLASS.default_state();
        let access = Cells(|x, y, z| match (x, y, z) {
            (6, 41, 6) => water(0),
            (7, 41, 6) => glass,
            _ => air(),
        });
        let mesh = mesh_of(pos, water(0), &access, MeshOptions::default());

        // Glass does not occlude: all six faces render, the east one with the
        // overlay material and no back face.
        assert_eq!(mesh.vertices.len(), 6 * 4);
        assert_eq!(mesh.translucent_indices.len(), 10 * 6);
    }

    fn slab(slab_type: SlabType) -> BlockState {
        (u16::from(Block::OAK_SLAB.min_state())..=u16::from(Block::OAK_SLAB.max_state()))
            .map(BlockState::from)
            .find(|state| state.slab_type() == slab_type && !state.is_waterlogged())
            .expect("slab state")
    }

    #[test]
    fn slab_below_matches_vanilla_down_face_rules() {
        let pos = [8, 42, 8];
        let top = slab(SlabType::Top);
        let access = Cells(|x, y, z| match (x, y, z) {
            (8, 42, 8) => water(0),
            (8, 41, 8) => top,
            _ => air(),
        });
        let mesh = mesh_of(pos, water(0), &access, MeshOptions::default());
        // A top slab reaches the cell ceiling and occludes the bottom face.
        assert_eq!(mesh.vertices.len(), 5 * 4);
        assert_eq!(mesh.translucent_indices.len(), 10 * 6);

        let bottom = slab(SlabType::Bottom);
        let access = Cells(|x, y, z| match (x, y, z) {
            (8, 42, 8) => water(0),
            (8, 41, 8) => bottom,
            _ => air(),
        });
        let mesh = mesh_of(pos, water(0), &access, MeshOptions::default());
        // A bottom slab stops halfway and leaves the bottom face visible.
        assert_eq!(mesh.vertices.len(), 6 * 4);
        assert_eq!(mesh.translucent_indices.len(), 11 * 6);
    }

    #[test]
    fn fluid_occlusion_matches_vanilla_probe() {
        // Baked from `occlusion_probe` (JVM): `FluidRenderer.isFaceOccludedByState`
        // replicated via `getFaceOcclusionShape` + `Shapes.blockOccludes` on
        // `box(0, 0, 0, 1, height, 1)`; bit per height {0.5, 0.8889, 1.0} and
        // columns DOWN, UP, NORTH, SOUTH, WEST, EAST.
        let probe_states = [
            BlockState::from(1),
            BlockState::from(661),
            BlockState::from(268),
            BlockState::from(5534),
            BlockState::from(8589),
            BlockState::from(8668),
            BlockState::from(8801),
            BlockState::from(10067),
            BlockState::from(13082),
            slab(SlabType::Top),
            slab(SlabType::Bottom),
            Block::POWDER_SNOW.default_state(),
            Block::COPPER_GRATE.default_state(),
        ];
        const PROBE_HEIGHTS: [f32; 3] = [0.5, 0.8888889, 1.0];
        const PROBE_OCCLUDED: [[u8; 6]; 13] = [
            [0x07, 0x04, 0x07, 0x07, 0x07, 0x07],
            [0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            [0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            [0x00, 0x04, 0x01, 0x01, 0x07, 0x01],
            [0x00, 0x04, 0x00, 0x00, 0x00, 0x00],
            [0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            [0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            [0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            [0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            [0x07, 0x00, 0x00, 0x00, 0x00, 0x00],
            [0x00, 0x04, 0x01, 0x01, 0x01, 0x01],
            [0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            [0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        ];
        let directions = [
            Direction::Down,
            Direction::Up,
            Direction::North,
            Direction::South,
            Direction::West,
            Direction::East,
        ];
        for (index, state) in probe_states.iter().enumerate() {
            let boxes = state.occlusion_boxes();
            for (dir_idx, direction) in directions.iter().enumerate() {
                for (height_idx, height) in PROBE_HEIGHTS.iter().enumerate() {
                    let expected = PROBE_OCCLUDED[index][dir_idx] >> height_idx & 1 == 1;
                    assert_eq!(
                        fluid_face_occluded(*direction, *height, boxes),
                        expected,
                        "state {} toward {direction:?} at height {height}",
                        u16::from(*state)
                    );
                }
            }
        }
    }

    #[test]
    fn lava_is_opaque() {
        let pos = [3, 30, 3];
        let lava = Block::LAVA.default_state();
        let access = Cells(|x, y, z| if [x, y, z] == pos { lava } else { air() });
        let mesh = mesh_of(pos, lava, &access, MeshOptions::default());

        assert!(mesh.translucent_indices.is_empty());
        assert_eq!(mesh.vertices.len(), 6 * 4);
        assert_eq!(mesh.indices.len(), 11 * 6);
    }
}
