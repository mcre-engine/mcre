//! Sodium-parity section culling: per-section face connectivity (vanilla
//! `VisGraph`), plus the occlusion-cull graph search
//! (Sodium `OcclusionCuller.findVisible`) combining distance, frustum and
//! BFS visibility traversal.
//!
//! Section visibility is computed at mesh time
//! ([`compute_section_visibility`]) and consumed per frame by
//! [`find_visible_sections`].

use std::collections::{HashMap, HashSet};

use mcre_core::Direction;
use mcre_world::{Chunk, SECTIONS_PER_CHUNK, WORLD_HEIGHT, WORLD_MIN_Y};

use crate::frustum::Frustum;

/// Face-connectivity bit-field for one 16³ section: the bit at
/// [`bit(from, to)`] is set when some path through the section's open cells
/// links the `from` face to the `to` face.
pub type SectionVisibility = u64;

/// Number of graph directions (Sodium `GraphDirection.COUNT`; vanilla
/// `Direction` ordinal order).
pub const DIRECTION_COUNT: usize = 6;

/// Bit position of the `from -> to` connection (Sodium
/// `VisibilityEncoding.bit`: 8 bits per `from` row so each row is a byte).
pub const fn bit(from: usize, to: usize) -> u32 {
    ((from * 8) + to) as u32
}

/// Every face reaches every face — the fail-open value for sections with
/// too few occluders to matter, for empty sections, and for mesh defaults
/// (Sodium `VisibilityEncoding` of `VisibilitySet.allTrue`).
pub const FULL_VISIBILITY: SectionVisibility = {
    let mut value = 0u64;
    let mut from = 0;
    while from < DIRECTION_COUNT {
        let mut to = 0;
        while to < DIRECTION_COUNT {
            value |= 1u64 << bit(from, to);
            to += 1;
        }
        from += 1;
    }
    value
};

/// Outgoing-direction set used when occlusion culling is off (every
/// direction traversable; Sodium `GraphDirectionSet.ALL`).
const ALL_CONNECTIONS: u32 = (1 << DIRECTION_COUNT) - 1;

const SECTION_LENGTH: i32 = 16;
const SECTION_VOLUME: usize = 16 * 16 * 16;
/// Vanilla `VisGraph`: sections with fewer opaque cells than this never
/// block view and short-circuit to [`FULL_VISIBILITY`].
const MIN_OPAQUE_FOR_FLOOD: usize = 256;

/// Sodium `OcclusionCuller.CHUNK_SECTION_SIZE`: section half-extent plus the
/// maximum model overhang (1 block) plus a frustum epsilon.
const FRUSTUM_HALF: f32 = 8.0 + 1.0 + 0.125;
/// The larger margin Sodium applies to the 3×3×3 sections around the camera
/// (`CHUNK_SECTION_SIZE_NEARBY`).
const NEARBY_FRUSTUM_HALF: f32 = 8.0 + 2.0 + 0.125;

const UP_DOWN_OCCLUDED: u64 = (1u64 << bit(0, 1)) | (1u64 << bit(1, 0));
const NORTH_SOUTH_OCCLUDED: u64 = (1u64 << bit(2, 3)) | (1u64 << bit(3, 2));
const WEST_EAST_OCCLUDED: u64 = (1u64 << bit(4, 5)) | (1u64 << bit(5, 4));

/// Culling inputs for one frame (Sodium passes render distance and the
/// occlusion toggle separately to `findVisible`).
#[derive(Clone, Copy, Debug)]
pub struct CullParams {
    /// Cylindrical search radius in blocks; sections beyond it are never
    /// visited (`f32::INFINITY` disables distance culling).
    pub search_distance: f32,
    /// When false only distance and frustum culling apply (Sodium's
    /// `useOcclusionCulling` off: traversal in any direction is legal).
    pub use_occlusion: bool,
}

impl Default for CullParams {
    fn default() -> Self {
        Self {
            search_distance: f32::INFINITY,
            use_occlusion: true,
        }
    }
}

/// Vanilla `VisGraph`: a 4096-cell bitset of opaque cells per section whose
/// flood fill over open cells yields the face-connectivity matrix.
struct VisGraph {
    opaque: [u64; SECTION_VOLUME / 64],
    empty: u32,
}

impl VisGraph {
    fn new() -> Self {
        Self {
            opaque: [0; SECTION_VOLUME / 64],
            empty: SECTION_VOLUME as u32,
        }
    }

    fn get(&self, index: usize) -> bool {
        self.opaque[index >> 6] >> (index & 63) & 1 == 1
    }

    fn set(&mut self, index: usize) {
        self.opaque[index >> 6] |= 1 << (index & 63);
    }

    /// Vanilla `VisGraph.setOpaque`, indexed by section-relative cells.
    fn set_opaque(&mut self, x: i32, y: i32, z: i32) {
        let index = cell_index((x & 15) as usize, (y & 15) as usize, (z & 15) as usize);
        if !self.get(index) {
            self.set(index);
            self.empty -= 1;
        }
    }

    /// Vanilla `VisGraph.resolve`: too few occluders means everything is
    /// visible, a full section means nothing is, otherwise every open edge
    /// cell floods its air pocket and records which faces it links.
    fn resolve(&mut self) -> SectionVisibility {
        let opaque_count = SECTION_VOLUME - self.empty as usize;
        if opaque_count < MIN_OPAQUE_FOR_FLOOD {
            return FULL_VISIBILITY;
        }
        if self.empty == 0 {
            return 0;
        }
        let mut data = 0u64;
        for_each_edge_cell(|index| {
            if !self.get(index) {
                let faces = self.flood_fill(index);
                for from in 0..DIRECTION_COUNT {
                    if faces & (1 << from) == 0 {
                        continue;
                    }
                    for to in 0..DIRECTION_COUNT {
                        if faces & (1 << to) != 0 {
                            data |= 1u64 << bit(from, to);
                        }
                    }
                }
            }
        });
        data
    }

    /// Vanilla `VisGraph.floodFill`: BFS through open cells (marking them
    /// opaque so pockets are flooded exactly once), collecting the section
    /// faces the pocket touches.
    fn flood_fill(&mut self, start: usize) -> u8 {
        let mut faces = 0u8;
        let mut queue = vec![start];
        self.set(start);
        let mut head = 0;
        while head < queue.len() {
            let index = queue[head];
            head += 1;
            let x = index & 15;
            let z = (index >> 4) & 15;
            let y = (index >> 8) & 15;
            if x == 0 {
                faces |= 1 << Direction::West.index();
            }
            if x == 15 {
                faces |= 1 << Direction::East.index();
            }
            if y == 0 {
                faces |= 1 << Direction::Down.index();
            }
            if y == 15 {
                faces |= 1 << Direction::Up.index();
            }
            if z == 0 {
                faces |= 1 << Direction::North.index();
            }
            if z == 15 {
                faces |= 1 << Direction::South.index();
            }
            for direction in Direction::ALL {
                let delta = direction.delta();
                let nx = x as i32 + delta[0];
                let ny = y as i32 + delta[1];
                let nz = z as i32 + delta[2];
                if !(0..SECTION_LENGTH).contains(&nx)
                    || !(0..SECTION_LENGTH).contains(&ny)
                    || !(0..SECTION_LENGTH).contains(&nz)
                {
                    continue;
                }
                let neighbor = cell_index(nx as usize, ny as usize, nz as usize);
                if !self.get(neighbor) {
                    self.set(neighbor);
                    queue.push(neighbor);
                }
            }
        }
        faces
    }
}

/// Vanilla `VisGraph.getIndex`: `x | z << 4 | y << 8`.
fn cell_index(x: usize, y: usize, z: usize) -> usize {
    x | (z << 4) | (y << 8)
}

fn for_each_edge_cell(mut visit: impl FnMut(usize)) {
    for x in 0..16usize {
        for z in 0..16usize {
            for y in 0..16usize {
                if x == 0 || x == 15 || z == 0 || z == 15 || y == 0 || y == 15 {
                    visit(cell_index(x, y, z));
                }
            }
        }
    }
}

/// Computes the per-section face-connectivity matrices for one chunk (the
/// data each section contributes to the occlusion-cull graph). A section's
/// cells are opaque when the state passes vanilla `isSolidRender` — the same
/// predicate vanilla's `SectionCompiler` feeds to `VisGraph`.
pub fn compute_section_visibility(chunk: &Chunk) -> [SectionVisibility; SECTIONS_PER_CHUNK] {
    let mut graphs: [VisGraph; SECTIONS_PER_CHUNK] = core::array::from_fn(|_| VisGraph::new());
    for (local_x, y, local_z, state) in chunk.iter_blocks() {
        if !state.solid_render() {
            continue;
        }
        let relative_y = y - WORLD_MIN_Y;
        let section = relative_y.div_euclid(SECTION_LENGTH);
        if !(0..SECTIONS_PER_CHUNK as i32).contains(&section) {
            continue;
        }
        graphs[section as usize].set_opaque(local_x, relative_y, local_z);
    }
    core::array::from_fn(|section| graphs[section].resolve())
}

/// BFS state: sections are enqueued once (touched) while their accumulated
/// incoming directions keep growing until the queue level is processed
/// (Sodium's `DoubleBufferedQueue` + `incomingDirections`).
struct Frontier {
    touched: HashSet<[i32; 3]>,
    incoming: HashMap<[i32; 3], u8>,
    queue: Vec<[i32; 3]>,
}

impl Frontier {
    fn new() -> Self {
        Self {
            touched: HashSet::new(),
            incoming: HashMap::new(),
            queue: Vec::new(),
        }
    }

    fn enqueue(&mut self, coord: [i32; 3], incoming: u8) {
        if !self.touched.contains(&coord) {
            self.touched.insert(coord);
            self.queue.push(coord);
        }
        *self.incoming.entry(coord).or_insert(0) |= incoming;
    }
}

/// Sodium `OcclusionCuller.findVisible`: walks the section graph outward
/// from the camera section, visiting every section reachable within the
/// search distance and frustum, then adds the 3×3×3 sections around the
/// camera whose extended bounds poke into the frustum (for large models).
///
/// `sections` maps section coordinates to their visibility data; missing
/// entries are unloaded/unknown sections that neither render nor traverse.
/// `visit` is called once per visible section; the return value is how many
/// sections were visited.
pub fn find_visible_sections(
    camera_pos: [f32; 3],
    frustum: &Frustum,
    params: CullParams,
    sections: &HashMap<[i32; 3], SectionVisibility>,
    mut visit: impl FnMut([i32; 3]),
) -> usize {
    let camera_int = [
        camera_pos[0].floor() as i32,
        camera_pos[1].floor() as i32,
        camera_pos[2].floor() as i32,
    ];
    let camera_frac = [
        camera_pos[0] - camera_int[0] as f32,
        camera_pos[1] - camera_int[1] as f32,
        camera_pos[2] - camera_int[2] as f32,
    ];
    let camera_section = [
        camera_int[0].div_euclid(SECTION_LENGTH),
        camera_int[1].div_euclid(SECTION_LENGTH),
        camera_int[2].div_euclid(SECTION_LENGTH),
    ];
    let min_section_y = WORLD_MIN_Y.div_euclid(SECTION_LENGTH);
    let max_section_y = (WORLD_MIN_Y + WORLD_HEIGHT).div_euclid(SECTION_LENGTH) - 1;

    let mut frontier = Frontier::new();
    let mut visited = 0usize;

    if camera_section[1] < min_section_y || camera_section[1] > max_section_y {
        // The camera sits outside the world: seed the boundary plane it
        // looks through with the camera-facing direction as incoming
        // (Sodium `initOutsideWorldHeight`), then search normally.
        let boundary = camera_section[1].clamp(min_section_y, max_section_y);
        let bound_incoming = if camera_section[1] > max_section_y {
            1 << Direction::Up.index()
        } else {
            1 << Direction::Down.index()
        };
        for &coord in sections.keys() {
            if coord[1] == boundary
                && frustum.intersects_box(section_center(coord), [FRUSTUM_HALF; 3])
            {
                frontier.enqueue(coord, bound_incoming);
            }
        }
    } else if sections.contains_key(&camera_section) {
        // The camera section is always visited and never frustum-tested
        // (Sodium `initWithinWorld`); with occlusion culling its outgoing
        // paths are any face pair connected inside it.
        frontier.touched.insert(camera_section);
        visit(camera_section);
        visited += 1;
        let visibility = sections[&camera_section];
        let mut connections = if params.use_occlusion {
            any_connections(visibility)
        } else {
            ALL_CONNECTIONS
        };
        connections &= outward(camera_section, camera_section);
        enqueue_neighbors(&mut frontier, sections, camera_section, connections);
    } else {
        // Camera section not meshed yet: seed whichever immediate neighbors
        // exist so the ring around the camera still renders.
        for direction in Direction::ALL {
            let neighbor = offset(camera_section, direction);
            if sections.contains_key(&neighbor) {
                frontier.enqueue(neighbor, 1 << direction.opposite().index());
            }
        }
    }

    while !frontier.queue.is_empty() {
        let current = std::mem::take(&mut frontier.queue);
        for coord in current {
            if !within_distance(coord, &camera_int, &camera_frac, params.search_distance)
                || !frustum.intersects_box(section_center(coord), [FRUSTUM_HALF; 3])
            {
                continue;
            }
            let Some(&visibility) = sections.get(&coord) else {
                continue;
            };
            visit(coord);
            visited += 1;
            let incoming = frontier.incoming.get(&coord).copied().unwrap_or(0);
            let mut connections = if params.use_occlusion {
                let masked = angle_masked(visibility, camera_pos, section_center(coord));
                connections_from(masked, incoming)
            } else {
                ALL_CONNECTIONS
            };
            connections &= outward(camera_section, coord);
            if connections != 0 {
                enqueue_neighbors(&mut frontier, sections, coord, connections);
            }
        }
    }

    // Sections around the camera that graph traversal cannot reach (or
    // drops at an angle) may still hold large models poking into the
    // frustum; render them under the bigger nearby margin
    // (Sodium `addNearbySections`).
    for dy in -1..=1 {
        for dx in -1..=1 {
            for dz in -1..=1 {
                if dx == 0 && dy == 0 && dz == 0 {
                    continue;
                }
                let coord = [
                    camera_section[0] + dx,
                    camera_section[1] + dy,
                    camera_section[2] + dz,
                ];
                if frontier.touched.contains(&coord) || !sections.contains_key(&coord) {
                    continue;
                }
                if !frustum.intersects_box(section_center(coord), [NEARBY_FRUSTUM_HALF; 3]) {
                    continue;
                }
                frontier.touched.insert(coord);
                visit(coord);
                visited += 1;
            }
        }
    }

    visited
}

fn enqueue_neighbors(
    frontier: &mut Frontier,
    sections: &HashMap<[i32; 3], SectionVisibility>,
    coord: [i32; 3],
    connections: u32,
) {
    for direction in Direction::ALL {
        if connections & (1 << direction.index()) == 0 {
            continue;
        }
        let neighbor = offset(coord, direction);
        if sections.contains_key(&neighbor) {
            frontier.enqueue(neighbor, 1 << direction.opposite().index());
        }
    }
}

fn offset(coord: [i32; 3], direction: Direction) -> [i32; 3] {
    let delta = direction.delta();
    [
        coord[0] + delta[0],
        coord[1] + delta[1],
        coord[2] + delta[2],
    ]
}

fn section_center(coord: [i32; 3]) -> [f32; 3] {
    [
        coord[0] as f32 * 16.0 + 8.0,
        coord[1] as f32 * 16.0 + 8.0,
        coord[2] as f32 * 16.0 + 8.0,
    ]
}

/// Sodium `OcclusionCuller.isWithinRenderDistance`: cylindrical distance to
/// the nearest point of the section's bounds expanded by one block (the
/// maximum model overhang).
fn within_distance(
    coord: [i32; 3],
    camera_int: &[i32; 3],
    camera_frac: &[f32; 3],
    max_distance: f32,
) -> bool {
    let mut squared = 0.0f32;
    for axis in 0..3 {
        let origin = coord[axis] * SECTION_LENGTH - camera_int[axis];
        let nearest = nearest_to_zero(origin - 1, origin + 17) as f32 - camera_frac[axis];
        if axis == 1 {
            if nearest.abs() >= max_distance {
                return false;
            }
        } else {
            squared += nearest * nearest;
        }
    }
    squared < max_distance * max_distance
}

fn nearest_to_zero(min: i32, max: i32) -> i32 {
    let mut clamped = 0;
    if min > 0 {
        clamped = min;
    }
    if max < 0 {
        clamped = max;
    }
    clamped
}

/// Sodium `OcclusionCuller.getAngleVisibilityMask`: a section viewed at a
/// steep angle cannot carry view through its opposite faces along the
/// foreshortened axes, so those through-pairs are masked off.
fn angle_masked(
    visibility: SectionVisibility,
    camera_pos: [f32; 3],
    center: [f32; 3],
) -> SectionVisibility {
    let dx = (camera_pos[0] - center[0]).abs();
    let dy = (camera_pos[1] - center[1]).abs();
    let dz = (camera_pos[2] - center[2]).abs();
    let mut occluded = 0u64;
    if dx > dy || dz > dy {
        occluded |= UP_DOWN_OCCLUDED;
    }
    if dx > dz || dy > dz {
        occluded |= NORTH_SOUTH_OCCLUDED;
    }
    if dy > dx || dz > dx {
        occluded |= WEST_EAST_OCCLUDED;
    }
    visibility & !occluded
}

/// Sodium `VisibilityEncoding.getConnections(visibility, incoming)`: the
/// union of the outgoing directions reachable from every incoming direction.
fn connections_from(visibility: SectionVisibility, incoming: u8) -> u32 {
    let mut outgoing = 0u32;
    for from in 0..DIRECTION_COUNT {
        if incoming & (1 << from) != 0 {
            outgoing |= ((visibility >> (from * 8)) & 0xFF) as u32;
        }
    }
    outgoing & ALL_CONNECTIONS
}

/// Sodium `VisibilityEncoding.getConnections(visibility)` (camera seed): any
/// outgoing direction with some path into it.
fn any_connections(visibility: SectionVisibility) -> u32 {
    let mut outgoing = 0u32;
    for from in 0..DIRECTION_COUNT {
        outgoing |= ((visibility >> (from * 8)) & 0xFF) as u32;
    }
    outgoing & ALL_CONNECTIONS
}

/// Sodium `OcclusionCuller.getOutwardDirections`: the graph search only
/// travels away from the camera section, so each axis allows the directions
/// pointing outward (both when the coordinate matches on that axis).
fn outward(camera: [i32; 3], coord: [i32; 3]) -> u32 {
    let mut planes = 0u32;
    planes |= if coord[0] <= camera[0] {
        1 << Direction::West.index()
    } else {
        0
    };
    planes |= if coord[0] >= camera[0] {
        1 << Direction::East.index()
    } else {
        0
    };
    planes |= if coord[1] <= camera[1] {
        1 << Direction::Down.index()
    } else {
        0
    };
    planes |= if coord[1] >= camera[1] {
        1 << Direction::Up.index()
    } else {
        0
    };
    planes |= if coord[2] <= camera[2] {
        1 << Direction::North.index()
    } else {
        0
    };
    planes |= if coord[2] >= camera[2] {
        1 << Direction::South.index()
    } else {
        0
    };
    planes
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcre_world::{Block, BlockState, ChunkPos};

    fn linked(visibility: SectionVisibility, from: Direction, to: Direction) -> bool {
        (visibility >> bit(from.index() as usize, to.index() as usize)) & 1 == 1
    }

    fn stone() -> BlockState {
        Block::STONE.default_state()
    }

    fn store(entries: &[([i32; 3], SectionVisibility)]) -> HashMap<[i32; 3], SectionVisibility> {
        entries.iter().copied().collect()
    }

    fn collect(
        camera_pos: [f32; 3],
        params: CullParams,
        sections: &HashMap<[i32; 3], SectionVisibility>,
    ) -> HashSet<[i32; 3]> {
        let frustum = Frustum::all();
        let mut visited = HashSet::new();
        find_visible_sections(camera_pos, &frustum, params, sections, |coord| {
            visited.insert(coord);
        });
        visited
    }

    fn default_params() -> CullParams {
        CullParams {
            search_distance: f32::INFINITY,
            use_occlusion: true,
        }
    }

    #[test]
    fn empty_section_is_fully_visible() {
        let chunk = Chunk::new_at(ChunkPos::new(0, 0));
        let visibility = compute_section_visibility(&chunk);
        assert_eq!(visibility, [FULL_VISIBILITY; SECTIONS_PER_CHUNK]);
    }

    #[test]
    fn solid_section_blocks_every_path() {
        let mut chunk = Chunk::new_at(ChunkPos::new(0, 0));
        for y in 0..16 {
            for z in 0..16 {
                for x in 0..16 {
                    chunk.set_block(x, y, z, stone());
                }
            }
        }
        let visibility = compute_section_visibility(&chunk);
        assert_eq!(visibility[4], 0);
        assert_eq!(visibility[3], FULL_VISIBILITY);
    }

    #[test]
    fn wall_severs_vertical_paths_but_not_lateral_ones() {
        let mut chunk = Chunk::new_at(ChunkPos::new(0, 0));
        for z in 0..16 {
            for x in 0..16 {
                chunk.set_block(x, 8, z, stone());
            }
        }
        let visibility = compute_section_visibility(&chunk)[4];
        assert!(!linked(visibility, Direction::Down, Direction::Up));
        assert!(!linked(visibility, Direction::Up, Direction::Down));
        assert!(linked(visibility, Direction::Down, Direction::North));
        assert!(linked(visibility, Direction::North, Direction::South));
        assert!(linked(visibility, Direction::Up, Direction::East));
    }

    #[test]
    fn wall_with_a_hole_keeps_vertical_paths() {
        let mut chunk = Chunk::new_at(ChunkPos::new(0, 0));
        for z in 0..16 {
            for x in 0..16 {
                if x != 0 {
                    chunk.set_block(x, 8, z, stone());
                }
            }
        }
        let visibility = compute_section_visibility(&chunk)[4];
        assert!(linked(visibility, Direction::Down, Direction::Up));
        assert!(linked(visibility, Direction::Up, Direction::Down));
    }

    #[test]
    fn sparse_section_short_circuits_to_fully_visible() {
        let mut chunk = Chunk::new_at(ChunkPos::new(0, 0));
        chunk.set_block(4, 8, 4, stone());
        let visibility = compute_section_visibility(&chunk);
        assert_eq!(visibility[4], FULL_VISIBILITY);
    }

    #[test]
    fn traverses_open_neighbors_from_the_camera_section() {
        let sections = store(&[
            ([0, 0, 0], FULL_VISIBILITY),
            ([1, 0, 0], FULL_VISIBILITY),
            ([0, 0, 1], FULL_VISIBILITY),
        ]);
        let visited = collect([8.0, 8.0, 8.0], default_params(), &sections);
        assert_eq!(visited.len(), 3);
        assert!(visited.contains(&[0, 0, 0]));
        assert!(visited.contains(&[1, 0, 0]));
        assert!(visited.contains(&[0, 0, 1]));
    }

    #[test]
    fn solid_section_stops_the_search() {
        let sections = store(&[
            ([0, 0, 0], FULL_VISIBILITY),
            ([1, 0, 0], 0),
            ([2, 0, 0], FULL_VISIBILITY),
        ]);
        let visited = collect([8.0, 8.0, 8.0], default_params(), &sections);
        assert!(visited.contains(&[1, 0, 0]));
        assert!(!visited.contains(&[2, 0, 0]));
    }

    #[test]
    fn occlusion_off_traverses_solid_sections() {
        let sections = store(&[
            ([0, 0, 0], FULL_VISIBILITY),
            ([1, 0, 0], 0),
            ([2, 0, 0], FULL_VISIBILITY),
        ]);
        let params = CullParams {
            search_distance: f32::INFINITY,
            use_occlusion: false,
        };
        let visited = collect([8.0, 8.0, 8.0], params, &sections);
        assert!(visited.contains(&[2, 0, 0]));
    }

    #[test]
    fn angle_mask_blocks_through_travel_on_foreshortened_axes() {
        let sections = store(&[
            ([0, 0, 0], FULL_VISIBILITY),
            ([1, 0, 0], FULL_VISIBILITY),
            ([2, 0, 0], FULL_VISIBILITY),
            ([2, 1, 0], FULL_VISIBILITY),
            ([2, 2, 0], FULL_VISIBILITY),
        ]);
        let visited = collect([8.0, 8.0, 8.0], default_params(), &sections);
        assert!(visited.contains(&[2, 1, 0]));
        assert!(!visited.contains(&[2, 2, 0]));

        let params = CullParams {
            search_distance: f32::INFINITY,
            use_occlusion: false,
        };
        let visited = collect([8.0, 8.0, 8.0], params, &sections);
        assert!(visited.contains(&[2, 2, 0]));
    }

    #[test]
    fn distance_limit_culls_sections_beyond_the_search_radius() {
        let sections = store(&[
            ([0, 0, 0], FULL_VISIBILITY),
            ([1, 0, 0], FULL_VISIBILITY),
            ([2, 0, 0], FULL_VISIBILITY),
        ]);
        let params = CullParams {
            search_distance: 20.0,
            use_occlusion: true,
        };
        let visited = collect([8.0, 8.0, 8.0], params, &sections);
        assert!(visited.contains(&[0, 0, 0]));
        assert!(visited.contains(&[1, 0, 0]));
        assert!(!visited.contains(&[2, 0, 0]));
    }

    #[test]
    fn camera_section_missing_seeds_present_neighbors() {
        let sections = store(&[([1, 0, 0], FULL_VISIBILITY)]);
        let visited = collect([8.0, 8.0, 8.0], default_params(), &sections);
        assert_eq!(visited.len(), 1);
        assert!(visited.contains(&[1, 0, 0]));
    }

    #[test]
    fn nearby_sections_render_around_a_blocked_camera() {
        let sections = store(&[([0, 0, 0], 0), ([1, 1, 1], FULL_VISIBILITY)]);
        let visited = collect([8.0, 8.0, 8.0], default_params(), &sections);
        assert!(visited.contains(&[0, 0, 0]));
        assert!(visited.contains(&[1, 1, 1]));
        assert_eq!(visited.len(), 2);
    }

    #[test]
    fn camera_above_world_seeds_the_top_boundary_plane() {
        let sections = store(&[
            ([0, 19, 0], FULL_VISIBILITY),
            ([0, 18, 0], FULL_VISIBILITY),
            ([0, 19, 1], FULL_VISIBILITY),
        ]);
        let visited = collect([8.0, 1000.0, 8.0], default_params(), &sections);
        assert!(visited.contains(&[0, 19, 0]));
        assert!(visited.contains(&[0, 18, 0]));
        assert!(visited.contains(&[0, 19, 1]));
    }

    #[test]
    fn outward_directions_only_point_away_from_the_camera() {
        assert_eq!(outward([0, 0, 0], [0, 0, 0]), ALL_CONNECTIONS);
        let west = outward([0, 0, 0], [-1, 0, 0]);
        assert_eq!(west & (1 << Direction::East.index()), 0);
        assert_ne!(west & (1 << Direction::West.index()), 0);
        let below = outward([0, 5, 0], [0, 0, 0]);
        assert_eq!(below & (1 << Direction::Up.index()), 0);
        assert_ne!(below & (1 << Direction::Down.index()), 0);
    }

    #[test]
    fn connections_fold_the_incoming_row() {
        let visibility = (1u64 << bit(0, 1)) | (1u64 << bit(1, 0));
        assert_eq!(
            connections_from(visibility, 1 << Direction::Down.index()),
            1 << Direction::Up.index()
        );
        assert_eq!(
            connections_from(visibility, 1 << Direction::North.index()),
            0
        );
        assert_eq!(
            any_connections(visibility),
            (1 << Direction::Down.index()) | (1 << Direction::Up.index())
        );
    }

    #[test]
    fn angle_mask_occludes_foreshortened_through_pairs() {
        let camera = [8.0, 8.0, 8.0];
        let masked = angle_masked(FULL_VISIBILITY, camera, [88.0, 8.0, 8.0]);
        assert!(linked(masked, Direction::West, Direction::East));
        assert!(!linked(masked, Direction::Down, Direction::Up));
        assert!(!linked(masked, Direction::North, Direction::South));
    }

    #[test]
    fn nearest_to_zero_clamps_toward_the_origin() {
        assert_eq!(nearest_to_zero(-1, 17), 0);
        assert_eq!(nearest_to_zero(5, 23), 5);
        assert_eq!(nearest_to_zero(-23, -5), -5);
    }
}
