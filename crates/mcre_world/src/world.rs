use alloc::collections::VecDeque;
use alloc::{boxed::Box, vec, vec::Vec};

use crate::light::{ALL_DIRECTIONS, ChunkLight, propagate};
use crate::{Biome, Block, BlockState, CHUNK_WIDTH, Chunk, ChunkPos, WORLD_HEIGHT, WORLD_MIN_Y};

const LIGHT_MARGIN: i32 = 16;
const LIGHT_TRACK_CAP: usize = 65_536;

pub trait WorldStorage {
    fn new(radius: u32) -> Self
    where
        Self: Sized;
    fn set(&mut self, pos: (i32, i32, i32), block: BlockState);
    fn get(&self, pos: (i32, i32, i32)) -> Option<BlockState>;
}

#[derive(Clone)]
pub struct World {
    radius: u32,
    chunks: Box<[Chunk]>,
    loaded: Box<[bool]>,
    dirty: Vec<ChunkPos>,
    evicted: Vec<ChunkPos>,
    light_edits: Vec<(i32, i32, i32, BlockState)>,
    light_full: bool,
}

impl World {
    pub fn new(radius: u32) -> Self {
        let num_chunks = ((2 * radius + 1) * (2 * radius + 1)) as usize;
        let mut chunks = vec![None; num_chunks];

        for x in (-(radius as i32))..=(radius as i32) {
            for z in (-(radius as i32))..=(radius as i32) {
                let idx = Self::chunk_idx(radius, (x, z));
                chunks[idx] = Some(Chunk::new_at(ChunkPos::new(x, z)));
            }
        }

        let chunks: Box<[Chunk]> = chunks.into_iter().map(|chunk| chunk.unwrap()).collect();

        Self {
            radius,
            chunks,
            loaded: vec![false; num_chunks].into_boxed_slice(),
            dirty: Vec::new(),
            evicted: Vec::new(),
            light_edits: Vec::new(),
            light_full: true,
        }
    }

    pub fn chunk_idx(radius: u32, chunk_pos: (i32, i32)) -> usize {
        let side_len = (2 * radius + 1) as i64;
        let x_relative = (chunk_pos.0 as i64).rem_euclid(side_len);
        let z_relative = (chunk_pos.1 as i64).rem_euclid(side_len);

        ((x_relative * side_len) + z_relative) as usize
    }

    pub const fn radius(&self) -> u32 {
        self.radius
    }

    pub fn set_block(&mut self, pos: (i32, i32, i32), block: BlockState) {
        let chunk_pos = ChunkPos::from_block(pos.0, pos.2);
        let chunk_idx = Self::chunk_idx(self.radius, (chunk_pos.x, chunk_pos.z));
        let chunk = &mut self.chunks[chunk_idx];
        if chunk.pos() != chunk_pos {
            return;
        }

        let local_x = pos.0.rem_euclid(16);
        let local_z = pos.2.rem_euclid(16);
        let old_state = chunk.get_block(local_x, pos.1, local_z);
        chunk.set_block(local_x, pos.1, local_z, block);
        self.mark_dirty(chunk_pos, local_x, local_z);
        self.track_light_edit(pos, old_state);
    }

    fn track_light_edit(&mut self, pos: (i32, i32, i32), old_state: BlockState) {
        if self.light_full {
            return;
        }
        if self.light_edits.len() < LIGHT_TRACK_CAP {
            self.light_edits.push((pos.0, pos.1, pos.2, old_state));
        } else {
            self.light_full = true;
            self.light_edits.clear();
        }
    }

    fn mark_dirty(&mut self, center: ChunkPos, local_x: i32, local_z: i32) {
        self.mark_chunk(center);
        if local_x == 0 {
            self.mark_chunk(ChunkPos::new(center.x - 1, center.z));
        }
        if local_x == CHUNK_WIDTH - 1 {
            self.mark_chunk(ChunkPos::new(center.x + 1, center.z));
        }
        if local_z == 0 {
            self.mark_chunk(ChunkPos::new(center.x, center.z - 1));
        }
        if local_z == CHUNK_WIDTH - 1 {
            self.mark_chunk(ChunkPos::new(center.x, center.z + 1));
        }
        // Corner blocks feed the diagonal neighbor's vertex AO/light taps.
        if (local_x == 0 || local_x == CHUNK_WIDTH - 1)
            && (local_z == 0 || local_z == CHUNK_WIDTH - 1)
        {
            let dx = if local_x == 0 { -1 } else { 1 };
            let dz = if local_z == 0 { -1 } else { 1 };
            self.mark_chunk(ChunkPos::new(center.x + dx, center.z + dz));
        }
    }

    fn mark_chunk(&mut self, pos: ChunkPos) {
        let idx = Self::chunk_idx(self.radius, (pos.x, pos.z));
        if self.chunks[idx].pos() != pos {
            return;
        }
        if !self.dirty.contains(&pos) {
            self.dirty.push(pos);
        }
    }

    pub fn take_dirty(&mut self) -> Vec<ChunkPos> {
        core::mem::take(&mut self.dirty)
    }

    /// Chunk positions whose slot data was overwritten by a different
    /// position (torus-ring aliasing) since the last call. Their meshes and
    /// their neighbors' baked borders are stale and must be dropped/remeshed.
    pub fn take_evicted(&mut self) -> Vec<ChunkPos> {
        core::mem::take(&mut self.evicted)
    }

    /// Records the position stored in `chunk_idx` as evicted when a loaded
    /// chunk occupies it under a different position.
    fn record_eviction(&mut self, chunk_idx: usize, new_pos: ChunkPos) -> Option<ChunkPos> {
        let old_pos = self.chunks[chunk_idx].pos();
        if self.loaded[chunk_idx] && old_pos != new_pos {
            self.evicted.push(old_pos);
            return Some(old_pos);
        }
        None
    }

    /// Positions with uncomputed light edits, in edit order.
    pub fn pending_light_edits(&self) -> Vec<(i32, i32, i32)> {
        self.light_edits
            .iter()
            .map(|edit| (edit.0, edit.1, edit.2))
            .collect()
    }

    /// Drops edit records whose light was recomputed elsewhere (worker
    /// results). Entries edited again afterwards are kept by the caller
    /// passing only the snapshot applied, so re-edits stay tracked.
    pub fn acknowledge_light_edits(&mut self, applied: &[(i32, i32, i32)]) {
        if self.light_full || applied.is_empty() {
            return;
        }
        self.light_edits
            .retain(|edit| !applied.contains(&(edit.0, edit.1, edit.2)));
    }

    pub fn set_chunk_light(&mut self, pos: ChunkPos, light: ChunkLight) -> bool {
        let idx = Self::chunk_idx(self.radius, (pos.x, pos.z));
        let chunk = &mut self.chunks[idx];
        if chunk.pos() != pos {
            return false;
        }
        *chunk.light_mut() = light;
        true
    }

    pub fn get_block(&self, pos: (i32, i32, i32)) -> Option<BlockState> {
        let chunk_pos = ChunkPos::from_block(pos.0, pos.2);
        let chunk_idx = Self::chunk_idx(self.radius, (chunk_pos.x, chunk_pos.z));
        let chunk = &self.chunks[chunk_idx];
        if chunk.pos() != chunk_pos {
            return None;
        }

        Some(chunk.get_block(pos.0.rem_euclid(16), pos.1, pos.2.rem_euclid(16)))
    }

    pub fn set_biome(&mut self, pos: (i32, i32, i32), biome: Biome) {
        let chunk_pos = ChunkPos::from_block(pos.0, pos.2);
        let chunk_idx = Self::chunk_idx(self.radius, (chunk_pos.x, chunk_pos.z));
        let chunk = &mut self.chunks[chunk_idx];
        if chunk.pos() != chunk_pos {
            return;
        }

        let local_x = pos.0.rem_euclid(16);
        let local_z = pos.2.rem_euclid(16);
        chunk.set_biome(local_x, pos.1, local_z, biome);
        self.mark_dirty(chunk_pos, local_x, local_z);
    }

    pub fn biome_at(&self, pos: (i32, i32, i32)) -> Biome {
        let chunk_pos = ChunkPos::from_block(pos.0, pos.2);
        let chunk_idx = Self::chunk_idx(self.radius, (chunk_pos.x, chunk_pos.z));
        let chunk = &self.chunks[chunk_idx];
        if chunk.pos() != chunk_pos {
            return Biome::PLAINS;
        }

        chunk.biome_at(pos.0.rem_euclid(16), pos.1, pos.2.rem_euclid(16))
    }

    fn chunk_at(&self, x: i32, z: i32) -> Option<&Chunk> {
        let chunk_pos = ChunkPos::from_block(x, z);
        let chunk = &self.chunks[Self::chunk_idx(self.radius, (chunk_pos.x, chunk_pos.z))];
        (chunk.pos() == chunk_pos).then_some(chunk)
    }

    fn chunk_at_mut(&mut self, x: i32, z: i32) -> Option<&mut Chunk> {
        let chunk_pos = ChunkPos::from_block(x, z);
        let chunk = &mut self.chunks[Self::chunk_idx(self.radius, (chunk_pos.x, chunk_pos.z))];
        (chunk.pos() == chunk_pos).then_some(chunk)
    }

    /// Sky light at a block position; unloaded positions default to 15 (open
    /// sky) so border faces stay lit before their neighbor chunk exists.
    pub fn sky_light(&self, pos: (i32, i32, i32)) -> u8 {
        self.chunk_at(pos.0, pos.2)
            .map(|c| {
                c.light()
                    .sky_level(pos.0.rem_euclid(16), pos.1, pos.2.rem_euclid(16))
            })
            .unwrap_or(15)
    }

    /// Block light at a block position; unloaded positions default to 0.
    pub fn block_light(&self, pos: (i32, i32, i32)) -> u8 {
        self.chunk_at(pos.0, pos.2)
            .map(|c| {
                c.light()
                    .block_level(pos.0.rem_euclid(16), pos.1, pos.2.rem_euclid(16))
            })
            .unwrap_or(0)
    }

    /// Combined `(block, sky)` light value associated with a block position;
    /// unloaded positions default to open sky (0, 15).
    pub fn light_level(&self, pos: (i32, i32, i32)) -> (u8, u8) {
        self.chunk_at(pos.0, pos.2)
            .map(|c| {
                c.light()
                    .level(pos.0.rem_euclid(16), pos.1, pos.2.rem_euclid(16))
            })
            .unwrap_or((0, 15))
    }

    /// Trusts the light that was imported with every loaded chunk instead of
    /// relighting the ring. Vanilla renders the light stored in the save (as
    /// long as it is marked on), so this is what the client sees; it is only
    /// skipped when a loaded chunk has no stored light or edits are pending.
    pub fn adopt_stored_light(&mut self) {
        let all_stored = self
            .loaded
            .iter()
            .zip(self.chunks.iter())
            .all(|(loaded, chunk)| !loaded || chunk.has_stored_light());
        if all_stored && self.light_edits.is_empty() {
            self.light_full = false;
        }
    }

    /// Recomputes sky and block light. After tracked edits only the
    /// affected region is recomputed; anything larger falls back to a
    /// whole-ring recompute. Both paths produce identical results.
    pub fn compute_light(&mut self) {
        if !self.light_full && self.light_edits.is_empty() {
            return;
        }
        if !self.light_full && self.light_edits.len() == 1 {
            let (x, y, z, old_state) = self.light_edits[0];
            self.update_light_at(x, y, z, old_state);
            self.light_edits.clear();
            return;
        }
        match self.light_region() {
            None => self.compute_light_full(),
            Some(region) => self.compute_light_incremental(region),
        }
    }

    fn light_region(&self) -> Option<(i32, i32, i32, i32)> {
        if self.light_full || self.light_edits.is_empty() {
            return None;
        }
        let mut min_x = i32::MAX;
        let mut max_x = i32::MIN;
        let mut min_z = i32::MAX;
        let mut max_z = i32::MIN;
        for &(x, _, z, _) in &self.light_edits {
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_z = min_z.min(z);
            max_z = max_z.max(z);
        }
        let min_x = min_x - LIGHT_MARGIN;
        let max_x = max_x + LIGHT_MARGIN;
        let min_z = min_z - LIGHT_MARGIN;
        let max_z = max_z + LIGHT_MARGIN;
        let region_cols = (max_x - min_x + 1) as i64 * (max_z - min_z + 1) as i64;
        let loaded_cols = self.chunks.len() as i64 * CHUNK_WIDTH as i64 * CHUNK_WIDTH as i64;
        if region_cols * 2 >= loaded_cols {
            return None;
        }
        Some((min_x, max_x, min_z, max_z))
    }

    /// Whole-ring recompute, bypassing edit tracking. Also serves as the
    /// differential oracle for the incremental paths in tests.
    pub fn compute_light_full(&mut self) {
        for chunk in &mut self.chunks {
            *chunk.light_mut() = ChunkLight::new();
        }

        let mut queue: VecDeque<(i32, i32, i32, u8, bool)> = VecDeque::new();
        let top_y = WORLD_MIN_Y + WORLD_HEIGHT - 1;
        for chunk in &self.chunks {
            let origin_x = chunk.pos().x * CHUNK_WIDTH;
            let origin_z = chunk.pos().z * CHUNK_WIDTH;
            for local_x in 0..CHUNK_WIDTH {
                for local_z in 0..CHUNK_WIDTH {
                    queue.push_back((origin_x + local_x, top_y, origin_z + local_z, 15, true));
                }
            }
            for &(local_x, y, local_z) in chunk.emitters() {
                let emission = chunk.get_block(local_x, y, local_z).light_emission();
                if emission > 0 {
                    queue.push_back((origin_x + local_x, y, origin_z + local_z, emission, false));
                }
            }
        }

        self.drain_light_queue(&mut queue, None);
        self.light_edits.clear();
        self.light_full = false;
    }

    fn compute_light_incremental(&mut self, region: (i32, i32, i32, i32)) {
        let (min_x, max_x, min_z, max_z) = region;
        for chunk in &mut self.chunks {
            let origin_x = chunk.pos().x * CHUNK_WIDTH;
            let origin_z = chunk.pos().z * CHUNK_WIDTH;
            if origin_x > max_x
                || origin_x + CHUNK_WIDTH - 1 < min_x
                || origin_z > max_z
                || origin_z + CHUNK_WIDTH - 1 < min_z
            {
                continue;
            }
            let lx0 = (min_x - origin_x).max(0);
            let lx1 = (max_x - origin_x).min(CHUNK_WIDTH - 1);
            let lz0 = (min_z - origin_z).max(0);
            let lz1 = (max_z - origin_z).min(CHUNK_WIDTH - 1);
            for local_x in lx0..=lx1 {
                for local_z in lz0..=lz1 {
                    for y in WORLD_MIN_Y..WORLD_MIN_Y + WORLD_HEIGHT {
                        chunk.light_mut().set_sky_level(local_x, y, local_z, 0);
                        chunk.light_mut().set_block_level(local_x, y, local_z, 0);
                    }
                }
            }
        }

        let mut queue: VecDeque<(i32, i32, i32, u8, bool)> = VecDeque::new();
        let top_y = WORLD_MIN_Y + WORLD_HEIGHT - 1;
        for x in min_x..=max_x {
            for z in min_z..=max_z {
                queue.push_back((x, top_y, z, 15, true));
            }
        }
        // Emitter seeds come from every chunk's incremental list (not a
        // block scan): out-of-region seeds are clipped by the drain bounds
        // at zero cost, while in-range ones flood exactly as in the full
        // path, including emitters just outside the region.
        for chunk in &self.chunks {
            let origin_x = chunk.pos().x * CHUNK_WIDTH;
            let origin_z = chunk.pos().z * CHUNK_WIDTH;
            for &(local_x, y, local_z) in chunk.emitters() {
                let emission = chunk.get_block(local_x, y, local_z).light_emission();
                if emission > 0 {
                    queue.push_back((origin_x + local_x, y, origin_z + local_z, emission, false));
                }
            }
        }
        for x in min_x..=max_x {
            for z in min_z..=max_z {
                if x != min_x && x != max_x && z != min_z && z != max_z {
                    continue;
                }
                for y in WORLD_MIN_Y..WORLD_MIN_Y + WORLD_HEIGHT {
                    let Some(state) = self.get_block((x, y, z)) else {
                        continue;
                    };
                    for direction in ALL_DIRECTIONS {
                        let delta = direction.delta();
                        if delta[1] != 0 {
                            continue;
                        }
                        let outside = (x - delta[0], y, z - delta[2]);
                        if outside.0 >= min_x
                            && outside.0 <= max_x
                            && outside.2 >= min_z
                            && outside.2 <= max_z
                        {
                            continue;
                        }
                        if self.chunk_at(outside.0, outside.2).is_none() {
                            continue;
                        }
                        let Some(outside_state) = self.get_block(outside) else {
                            continue;
                        };
                        for sky in [true, false] {
                            let incoming = if sky {
                                self.sky_light(outside)
                            } else {
                                self.block_light(outside)
                            };
                            if incoming == 0 {
                                continue;
                            }
                            let next = propagate(incoming, outside_state, state, direction, sky);
                            if next > 0 {
                                queue.push_back((x, y, z, next, sky));
                            }
                        }
                    }
                }
            }
        }

        self.drain_light_queue(&mut queue, Some(region));
        self.light_edits.clear();
        self.light_full = false;
    }

    /// Targeted relight after a single tracked edit. The block at `pos` is
    /// already updated; light arrays still hold pre-edit values. Only cells
    /// whose light actually changes are visited, so this is O(affected)
    /// instead of O(region). Falls back to nothing when the recomputed level
    /// matches, which covers the common no-op edit.
    fn update_light_at(&mut self, x: i32, y: i32, z: i32, old_state: BlockState) {
        if !(WORLD_MIN_Y..WORLD_MIN_Y + WORLD_HEIGHT).contains(&y) {
            return;
        }
        let Some(state) = self.get_block((x, y, z)) else {
            return;
        };
        for sky in [false, true] {
            self.update_channel_at(x, y, z, state, old_state, sky);
        }
    }

    fn channel_value(&self, x: i32, y: i32, z: i32, sky: bool) -> u8 {
        if sky {
            self.sky_light((x, y, z))
        } else {
            self.block_light((x, y, z))
        }
    }

    fn set_channel_value(&mut self, x: i32, y: i32, z: i32, sky: bool, level: u8) {
        let Some(chunk) = self.chunk_at_mut(x, z) else {
            return;
        };
        let (local_x, local_z) = (x.rem_euclid(16), z.rem_euclid(16));
        if sky {
            chunk.light_mut().set_sky_level(local_x, y, local_z, level);
        } else {
            chunk
                .light_mut()
                .set_block_level(local_x, y, local_z, level);
        }
    }

    fn update_channel_at(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        state: BlockState,
        old_state: BlockState,
        sky: bool,
    ) {
        let old = self.channel_value(x, y, z, sky);
        let mut incoming = if sky { 0 } else { state.light_emission() };
        for direction in ALL_DIRECTIONS {
            let to_neighbor = direction.delta();
            let (nx, ny, nz) = (x + to_neighbor[0], y + to_neighbor[1], z + to_neighbor[2]);
            // Unloaded neighbors feed nothing, mirroring drain_light_queue:
            // reads there use accessor defaults only inside loaded columns.
            let Some(neighbor_state) = self.get_block((nx, ny, nz)) else {
                continue;
            };
            let feed = self.channel_value(nx, ny, nz, sky);
            if feed == 0 {
                continue;
            }
            incoming = incoming.max(propagate(
                feed,
                neighbor_state,
                state,
                direction.opposite(),
                sky,
            ));
        }
        if incoming == old && state == old_state {
            return;
        }

        if incoming < old {
            self.remove_channel_at(x, y, z, old, sky);
        }

        // A state change alters how this cell feeds its neighbors even when
        // its own level is unchanged (shape occlusion). Compare pre-edit and
        // post-edit feed per direction: increased feeds push, and neighbors
        // that could only have been fed by the old feed are cleared and
        // reseeded.
        let x_value = if incoming > old {
            incoming
        } else {
            self.channel_value(x, y, z, sky)
        };
        let mut queue = VecDeque::new();
        if incoming > old {
            queue.push_back((x, y, z, incoming, sky));
        }
        for direction in ALL_DIRECTIONS {
            let delta = direction.delta();
            let (nx, ny, nz) = (x + delta[0], y + delta[1], z + delta[2]);
            if !(WORLD_MIN_Y..WORLD_MIN_Y + WORLD_HEIGHT).contains(&ny) {
                continue;
            }
            let Some(neighbor_state) = self.get_block((nx, ny, nz)) else {
                continue;
            };
            let current = self.channel_value(nx, ny, nz, sky);
            let old_feed = propagate(old, old_state, neighbor_state, direction, sky);
            let new_feed = propagate(x_value, state, neighbor_state, direction, sky);
            if new_feed > current {
                queue.push_back((nx, ny, nz, new_feed, sky));
            } else if new_feed < current && current <= old_feed {
                self.remove_channel_at(nx, ny, nz, current, sky);
            }
        }
        self.drain_light_queue(&mut queue, None);
    }

    /// Removal wave for a light decrease at `(x, y, z)`: clears cells that
    /// could only have been fed through the removed light, then reseeds the
    /// cleared cells from surviving neighbors and re-propagates to fixpoint.
    fn remove_channel_at(&mut self, x: i32, y: i32, z: i32, removed: u8, sky: bool) {
        self.set_channel_value(x, y, z, sky, 0);
        let mut wave = VecDeque::from([((x, y, z), removed)]);
        let mut cleared = vec![(x, y, z)];
        while let Some(((cx, cy, cz), removed_level)) = wave.pop_front() {
            let Some(source) = self.get_block((cx, cy, cz)) else {
                continue;
            };
            for direction in ALL_DIRECTIONS {
                let delta = direction.delta();
                let (nx, ny, nz) = (cx + delta[0], cy + delta[1], cz + delta[2]);
                if !(WORLD_MIN_Y..WORLD_MIN_Y + WORLD_HEIGHT).contains(&ny) {
                    continue;
                }
                let value = self.channel_value(nx, ny, nz, sky);
                if value == 0 {
                    continue;
                }
                let Some(neighbor) = self.get_block((nx, ny, nz)) else {
                    continue;
                };
                if value <= propagate(removed_level, source, neighbor, direction, sky) {
                    self.set_channel_value(nx, ny, nz, sky, 0);
                    cleared.push((nx, ny, nz));
                    wave.push_back(((nx, ny, nz), value));
                }
            }
        }
        let mut queue = VecDeque::new();
        for (cx, cy, cz) in cleared {
            let Some(cleared_state) = self.get_block((cx, cy, cz)) else {
                continue;
            };
            if !sky {
                let emission = cleared_state.light_emission();
                if emission > 0 {
                    queue.push_back((cx, cy, cz, emission, false));
                }
            }
            for direction in ALL_DIRECTIONS {
                let delta = direction.delta();
                let (nx, ny, nz) = (cx + delta[0], cy + delta[1], cz + delta[2]);
                if !(WORLD_MIN_Y..WORLD_MIN_Y + WORLD_HEIGHT).contains(&ny) {
                    continue;
                }
                let Some(neighbor) = self.get_block((nx, ny, nz)) else {
                    continue;
                };
                let feed = self.channel_value(nx, ny, nz, sky);
                if feed == 0 {
                    continue;
                }
                let next = propagate(feed, neighbor, cleared_state, direction.opposite(), sky);
                if next > 0 {
                    queue.push_back((cx, cy, cz, next, sky));
                }
            }
        }
        self.drain_light_queue(&mut queue, None);
    }

    fn drain_light_queue(
        &mut self,
        queue: &mut VecDeque<(i32, i32, i32, u8, bool)>,
        bounds: Option<(i32, i32, i32, i32)>,
    ) {
        while let Some((x, y, z, level, sky)) = queue.pop_front() {
            if !((WORLD_MIN_Y..WORLD_MIN_Y + WORLD_HEIGHT).contains(&y)) {
                continue;
            }
            if let Some((min_x, max_x, min_z, max_z)) = bounds
                && (x < min_x || x > max_x || z < min_z || z > max_z)
            {
                continue;
            }
            let current = if sky {
                self.sky_light((x, y, z))
            } else {
                self.block_light((x, y, z))
            };
            if level <= current {
                continue;
            }
            let Some(chunk) = self.chunk_at_mut(x, z) else {
                continue;
            };
            let local_x = x.rem_euclid(16);
            let local_z = z.rem_euclid(16);
            if sky {
                chunk.light_mut().set_sky_level(local_x, y, local_z, level);
            } else {
                chunk
                    .light_mut()
                    .set_block_level(local_x, y, local_z, level);
            }
            let Some(from_state) = self.get_block((x, y, z)) else {
                continue;
            };

            for direction in ALL_DIRECTIONS {
                let delta = direction.delta();
                let neighbor = (x + delta[0], y + delta[1], z + delta[2]);
                if !((WORLD_MIN_Y..WORLD_MIN_Y + WORLD_HEIGHT).contains(&neighbor.1)) {
                    continue;
                }
                if let Some((min_x, max_x, min_z, max_z)) = bounds
                    && (neighbor.0 < min_x
                        || neighbor.0 > max_x
                        || neighbor.2 < min_z
                        || neighbor.2 > max_z)
                {
                    continue;
                }
                let Some(state) = self.get_block(neighbor) else {
                    continue;
                };
                let next = propagate(level, from_state, state, direction, sky);
                if next > 0 {
                    queue.push_back((neighbor.0, neighbor.1, neighbor.2, next, sky));
                }
            }
        }
    }

    pub fn optimize(&mut self) {
        for chunk in &mut self.chunks {
            chunk.optimize();
        }
    }

    pub fn add_chunk(&mut self, chunk: Chunk) {
        let chunk_idx = Self::chunk_idx(self.radius, (chunk.pos().x, chunk.pos().z));
        self.record_eviction(chunk_idx, chunk.pos());
        self.chunks[chunk_idx] = chunk;
        self.loaded[chunk_idx] = true;
        self.light_full = true;
        self.light_edits.clear();
    }

    /// Replaces the chunk slot for `chunk.pos()` without forcing a full
    /// relight: the chunk and its border neighbors are marked mesh-dirty and
    /// two opposite corner columns seed the incremental light region, which
    /// covers the whole chunk plus the propagation margin. (One seed would
    /// take the single-cell fast path, which is only valid when surrounding
    /// light is already consistent — not the case for freshly loaded
    /// chunks.) Used by the viewer streamer; the worker computes the light
    /// afterwards.
    pub fn replace_chunk(&mut self, chunk: Chunk) {
        let pos = chunk.pos();
        let chunk_idx = Self::chunk_idx(self.radius, (pos.x, pos.z));
        let evicted = self.record_eviction(chunk_idx, pos);
        self.chunks[chunk_idx] = chunk;
        self.loaded[chunk_idx] = true;
        // All eight neighbors: vertex AO/light taps reach diagonally, so a
        // fresh chunk changes its diagonal neighbors' baked corners too.
        for dz in -1..=1 {
            for dx in -1..=1 {
                self.mark_chunk(ChunkPos::new(pos.x + dx, pos.z + dz));
            }
        }
        if let Some(old_pos) = evicted {
            // The evicted chunk's mesh must be dropped and its neighbors
            // remeshed: their baked borders still sampled data that just
            // vanished from the ring.
            for dz in -1..=1 {
                for dx in -1..=1 {
                    self.mark_chunk(ChunkPos::new(old_pos.x + dx, old_pos.z + dz));
                }
            }
        }
        // Chunks loaded from the save bring their own (already consistent)
        // light, which the vanilla client renders too, so they must not be
        // relit. Freshly built chunks still need the relight seeds.
        if !self.chunks[chunk_idx].has_stored_light() {
            if !self.light_full && self.light_edits.len() + 2 < LIGHT_TRACK_CAP {
                let (x0, z0) = (pos.x * CHUNK_WIDTH, pos.z * CHUNK_WIDTH);
                self.light_edits
                    .push((x0, 0, z0, Block::AIR.default_state()));
                self.light_edits.push((
                    x0 + CHUNK_WIDTH - 1,
                    0,
                    z0 + CHUNK_WIDTH - 1,
                    Block::AIR.default_state(),
                ));
            } else if !self.light_full {
                self.light_full = true;
                self.light_edits.clear();
            }
        }
    }

    /// Chunk stored for chunk position `pos`, or `None` if its slot holds
    /// a different position (torus ring: positions outside the ring alias
    /// slots but never match).
    pub fn chunk_at_pos(&self, pos: ChunkPos) -> Option<&Chunk> {
        let chunk = &self.chunks[Self::chunk_idx(self.radius, (pos.x, pos.z))];
        (chunk.pos() == pos).then_some(chunk)
    }

    /// Whether `pos` holds real chunk data (imported, streamed, or added)
    /// rather than the empty placeholder `new` fills the ring with. The
    /// streamer uses this to tell "not loaded yet" from "loaded and empty".
    pub fn is_chunk_loaded(&self, pos: ChunkPos) -> bool {
        let idx = Self::chunk_idx(self.radius, (pos.x, pos.z));
        self.loaded[idx] && self.chunks[idx].pos() == pos
    }

    /// Raw slot replace with no dirty or light bookkeeping. Used when the
    /// caller applies worker-computed light/meshes itself.
    pub fn set_chunk(&mut self, chunk: Chunk) {
        let pos = chunk.pos();
        let chunk_idx = Self::chunk_idx(self.radius, (pos.x, pos.z));
        self.record_eviction(chunk_idx, pos);
        self.chunks[chunk_idx] = chunk;
        self.loaded[chunk_idx] = true;
    }

    /// Marks every chunk mesh-dirty (debug "Remesh" path).
    pub fn mark_all_dirty(&mut self) {
        self.dirty.clear();
        let positions: Vec<ChunkPos> = self.iter_chunks().map(Chunk::pos).collect();
        self.dirty.extend(positions);
    }

    /// Forces a whole-ring relight on the next `compute_light` (debug
    /// "Recompute light" path).
    pub fn mark_light_full(&mut self) {
        self.light_full = true;
        self.light_edits.clear();
    }

    pub fn iter_chunks(&self) -> impl Iterator<Item = &Chunk> {
        self.chunks.iter()
    }
}

impl WorldStorage for World {
    fn new(radius: u32) -> Self {
        Self::new(radius)
    }

    fn set(&mut self, pos: (i32, i32, i32), block: BlockState) {
        self.set_block(pos, block);
    }

    fn get(&self, pos: (i32, i32, i32)) -> Option<BlockState> {
        self.get_block(pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_chunk_marks_dirty_and_seeds_without_full() {
        let mut world = World::new(1);
        world.compute_light();
        assert!(world.take_dirty().is_empty());
        assert!(world.pending_light_edits().is_empty());

        world.replace_chunk(Chunk::new_at(ChunkPos::new(0, 0)));
        assert!(!world.light_full);
        assert_eq!(world.pending_light_edits().len(), 2);
        let dirty = world.take_dirty();
        assert_eq!(dirty.len(), 9);
        for dz in -1..=1 {
            for dx in -1..=1 {
                assert!(dirty.contains(&ChunkPos::new(dx, dz)));
            }
        }
    }

    #[test]
    fn replace_chunk_records_alias_evictions_and_marks_their_border() {
        let mut world = World::new(1);
        world.compute_light();

        // (1, 0) and (-2, 0) share a ring slot (side 3).
        world.replace_chunk(Chunk::new_at(ChunkPos::new(1, 0)));
        assert!(world.take_evicted().is_empty());
        let _ = world.take_dirty();

        world.replace_chunk(Chunk::new_at(ChunkPos::new(-2, 0)));
        assert_eq!(world.take_evicted(), vec![ChunkPos::new(1, 0)]);
        let dirty = world.take_dirty();
        // (0, 0) and (0, 1) are victim-only neighbors: their baked borders
        // sampled the data that just vanished, so they must be remeshed.
        assert!(dirty.contains(&ChunkPos::new(0, 0)));
        assert!(dirty.contains(&ChunkPos::new(0, 1)));
        // The victim's own position is gone from the ring.
        assert!(!dirty.contains(&ChunkPos::new(1, 0)));
    }

    #[test]
    fn set_chunk_records_alias_evictions_without_dirty_marks() {
        let mut world = World::new(1);
        world.replace_chunk(Chunk::new_at(ChunkPos::new(1, 0)));
        let _ = world.take_evicted();
        let _ = world.take_dirty();

        world.set_chunk(Chunk::new_at(ChunkPos::new(-2, 0)));
        assert_eq!(world.take_evicted(), vec![ChunkPos::new(1, 0)]);
        assert!(world.take_dirty().is_empty());
    }

    #[test]
    fn corner_block_edit_marks_diagonal_neighbor() {
        use crate::Block;
        let mut world = World::new(1);
        world.compute_light();
        assert!(world.take_dirty().is_empty());

        world.set_block((0, 0, 0), Block::STONE.default_state());
        let dirty = world.take_dirty();
        assert!(dirty.contains(&ChunkPos::new(0, 0)));
        assert!(dirty.contains(&ChunkPos::new(-1, 0)));
        assert!(dirty.contains(&ChunkPos::new(0, -1)));
        assert!(dirty.contains(&ChunkPos::new(-1, -1)));
        assert_eq!(dirty.len(), 4);

        world.set_block((8, 0, 8), Block::STONE.default_state());
        assert_eq!(world.take_dirty(), vec![ChunkPos::new(0, 0)]);
    }

    #[test]
    fn set_chunk_is_bookkeeping_free() {
        let mut world = World::new(0);
        world.compute_light();
        world.take_dirty();
        world.set_chunk(Chunk::new_at(ChunkPos::new(0, 0)));
        assert!(world.take_dirty().is_empty());
        assert!(world.pending_light_edits().is_empty());
        assert!(!world.light_full);
    }

    #[test]
    fn placeholders_are_unloaded_until_chunk_data_arrives() {
        let mut world = World::new(1);
        for pos in [ChunkPos::new(0, 0), ChunkPos::new(1, 1)] {
            assert!(world.chunk_at_pos(pos).is_some());
            assert!(!world.is_chunk_loaded(pos));
        }

        world.replace_chunk(Chunk::new_at(ChunkPos::new(0, 0)));
        assert!(world.is_chunk_loaded(ChunkPos::new(0, 0)));
        assert!(!world.is_chunk_loaded(ChunkPos::new(1, 1)));

        world.add_chunk(Chunk::new_at(ChunkPos::new(1, 1)));
        assert!(world.is_chunk_loaded(ChunkPos::new(1, 1)));

        world.set_chunk(Chunk::new_at(ChunkPos::new(-1, 0)));
        assert!(world.is_chunk_loaded(ChunkPos::new(-1, 0)));
    }

    #[test]
    fn streamed_chunk_light_matches_full() {
        use crate::Block;
        fn platform_chunk(x: i32, z: i32) -> Chunk {
            let mut chunk = Chunk::new_at(ChunkPos::new(x, z));
            for lx in 0..16 {
                for lz in 0..16 {
                    chunk.set_block(lx, 60 + ((lx + lz) % 5), lz, Block::STONE.default_state());
                }
            }
            chunk
        }
        // Radius 2 keeps the incremental region below the full-fallback
        // guard (region*2 < loaded), so this exercises the region path.
        let mut world = World::new(2);
        for x in -2..=2 {
            for z in -2..=2 {
                world.set_chunk(platform_chunk(x, z));
            }
        }
        world.compute_light_full();
        world.replace_chunk(platform_chunk(0, 0));
        world.compute_light();
        let mut oracle = World::new(2);
        for x in -2..=2 {
            for z in -2..=2 {
                oracle.set_chunk(platform_chunk(x, z));
            }
        }
        oracle.compute_light_full();
        for chunk in world.iter_chunks() {
            let (ox, oz) = (chunk.pos().x * 16, chunk.pos().z * 16);
            for lx in 0..16 {
                for lz in 0..16 {
                    for y in [61, 64, 70, 100, 200] {
                        assert_eq!(
                            world.light_level((ox + lx, y, oz + lz)),
                            oracle.light_level((ox + lx, y, oz + lz)),
                            "stream light mismatch at ({}, {y}, {})",
                            ox + lx,
                            oz + lz,
                        );
                    }
                }
            }
        }
    }
}
