//! Sodium-parity render regions: 8×4×8 section groups with a shared GPU
//! vertex arena (best-fit free-list, grow-by-copy) and per-section
//! per-pass facing-run metadata. Also the global shared quad index buffer.

use std::collections::HashMap;

use mcre_world::WORLD_MIN_Y;
use wgpu::util::DeviceExt;

use crate::mesh::{QuadSortData, SectionMesh};
use crate::vertex::{
    FACING_COUNT, FACING_NEG_X, FACING_NEG_Y, FACING_NEG_Z, FACING_POS_X, FACING_POS_Y,
    FACING_POS_Z, FACING_UNASSIGNED, TerrainPass, TerrainVertex,
};

pub type RegionKey = (i32, i32, i32);

pub const SECTIONS_PER_REGION: usize = 8 * 4 * 8;
pub const VERTEX_STRIDE: u64 = size_of::<TerrainVertex>() as u64;
const FREE_LAG_FRAMES: u64 = 3;
const INITIAL_ARENA_BYTES: u64 = 1 << 20;
const INITIAL_QUADS: u32 = 1 << 15;

pub fn region_key(section_x: i32, section_y: i32, section_z: i32) -> RegionKey {
    (
        section_x.div_euclid(8),
        section_y.div_euclid(4),
        section_z.div_euclid(8),
    )
}

/// Sodium `LocalSectionIndex`: x in bits 0..2, z in bits 3..5, y in bits 6..7.
pub fn local_section_index(section_x: i32, section_y: i32, section_z: i32) -> usize {
    let x = section_x.rem_euclid(8) as usize;
    let y = section_y.rem_euclid(4) as usize;
    let z = section_z.rem_euclid(8) as usize;
    x | z << 3 | y << 6
}

/// Region key and local slot for chunk-local section `y` (0..24) of the chunk
/// column at block `origin`.
pub fn chunk_section_slot(origin: [i32; 2], section: usize) -> (RegionKey, usize) {
    let section_x = origin[0].div_euclid(16);
    let section_z = origin[1].div_euclid(16);
    let section_y = WORLD_MIN_Y.div_euclid(16) + section as i32;
    (
        region_key(section_x, section_y, section_z),
        local_section_index(section_x, section_y, section_z),
    )
}

/// World-space block origin of a chunk-local section.
pub fn section_world_origin(origin: [i32; 2], section: usize) -> [i32; 3] {
    [origin[0], WORLD_MIN_Y + section as i32 * 16, origin[1]]
}

/// Sodium `getVisibleFaces`: 7-bit mask of facing slices that can be seen
/// from `camera`, with a ±3-block epsilon for model overhang. Bit `f` is set
/// when facing `f` (`vertex::FACING_*`) may be visible.
pub fn visible_faces(camera: [f32; 3], section_origin: [i32; 3]) -> u32 {
    const EPSILON: f32 = 3.0;
    let min = [
        section_origin[0] as f32,
        section_origin[1] as f32,
        section_origin[2] as f32,
    ];
    let max = [min[0] + 16.0, min[1] + 16.0, min[2] + 16.0];
    let mut mask = 1u32 << FACING_UNASSIGNED;
    if camera[0] > min[0] - EPSILON {
        mask |= 1 << FACING_POS_X;
    }
    if camera[0] < max[0] + EPSILON {
        mask |= 1 << FACING_NEG_X;
    }
    if camera[1] > min[1] - EPSILON {
        mask |= 1 << FACING_POS_Y;
    }
    if camera[1] < max[1] + EPSILON {
        mask |= 1 << FACING_NEG_Y;
    }
    if camera[2] > min[2] - EPSILON {
        mask |= 1 << FACING_POS_Z;
    }
    if camera[2] < max[2] + EPSILON {
        mask |= 1 << FACING_NEG_Z;
    }
    mask
}

/// Slice mask for a pass: bit `f` set when facing `f` has geometry.
pub fn slice_mask(counts: &[u32; FACING_COUNT]) -> u32 {
    let mut mask = 0u32;
    for (facing, count) in counts.iter().enumerate() {
        if *count > 0 {
            mask |= 1 << facing;
        }
    }
    mask
}

/// Sodium `addSharedIndexedDrawCommands`: merges consecutive visible facing
/// runs of one section pass into `(base_vertex, vertex_count)` draw ranges
/// relative to the pass base.
pub fn merge_visible_runs(
    counts: &[u32; FACING_COUNT],
    pass_base: u32,
    visible: u32,
) -> Vec<(u32, u32)> {
    let mut runs = Vec::new();
    let mut facing = 0;
    while facing < FACING_COUNT {
        if visible & (1 << facing) == 0 || counts[facing] == 0 {
            facing += 1;
            continue;
        }
        let start = facing;
        let mut vertex_count = 0u32;
        while facing < FACING_COUNT && visible & (1 << facing) != 0 && counts[facing] != 0 {
            vertex_count += counts[facing];
            facing += 1;
        }
        let mut base_vertex = pass_base;
        for count in &counts[..start] {
            base_vertex += count;
        }
        runs.push((base_vertex, vertex_count));
    }
    runs
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FreeBlock {
    offset: u64,
    size: u64,
}

/// Pure byte-offset allocator over a fixed capacity: best-fit free list with
/// adjacent-block coalescing. Frees are lagged by [`FREE_LAG_FRAMES`] frames
/// so in-flight draws never read reused vertex data.
#[derive(Debug)]
struct FreeList {
    free: Vec<FreeBlock>,
    pending: Vec<(u64, FreeBlock)>,
    frame: u64,
    used: u64,
    capacity: u64,
}

impl FreeList {
    fn new(capacity: u64) -> Self {
        Self {
            free: vec![FreeBlock {
                offset: 0,
                size: capacity,
            }],
            pending: Vec::new(),
            frame: 0,
            used: 0,
            capacity,
        }
    }

    fn alloc(&mut self, size: u64) -> Option<u64> {
        let mut best: Option<usize> = None;
        for (i, block) in self.free.iter().enumerate() {
            if block.size >= size && best.is_none_or(|b| self.free[b].size > block.size) {
                best = Some(i);
            }
        }
        let i = best?;
        let block = self.free[i];
        let offset = block.offset;
        if block.size == size {
            self.free.remove(i);
        } else {
            self.free[i] = FreeBlock {
                offset: block.offset + size,
                size: block.size - size,
            };
        }
        self.used += size;
        Some(offset)
    }

    fn free(&mut self, offset: u64, size: u64) {
        self.pending.push((self.frame, FreeBlock { offset, size }));
        self.used -= size;
    }

    fn begin_frame(&mut self) {
        self.frame += 1;
        let ready: Vec<FreeBlock> = {
            let (ready, rest): (Vec<_>, Vec<_>) = self
                .pending
                .drain(..)
                .partition(|(frame, _)| frame.saturating_add(FREE_LAG_FRAMES) <= self.frame);
            self.pending = rest;
            ready.into_iter().map(|(_, block)| block).collect()
        };
        for block in ready {
            self.insert(block);
        }
    }

    fn insert(&mut self, block: FreeBlock) {
        let at = self.free.partition_point(|b| b.offset < block.offset);
        let mut block = block;
        if at > 0 && self.free[at - 1].offset + self.free[at - 1].size == block.offset {
            let prev = self.free.remove(at - 1);
            block = FreeBlock {
                offset: prev.offset,
                size: prev.size + block.size,
            };
        }
        let at = self.free.partition_point(|b| b.offset < block.offset);
        if at < self.free.len() && block.offset + block.size == self.free[at].offset {
            let next = self.free.remove(at);
            block.size += next.size;
        }
        self.free.insert(
            self.free.partition_point(|b| b.offset < block.offset),
            block,
        );
    }

    fn grow(&mut self, capacity: u64) {
        let extra = capacity - self.capacity;
        if let Some(last) = self.free.last_mut()
            && last.offset + last.size == self.capacity
        {
            last.size += extra;
        } else {
            self.free.push(FreeBlock {
                offset: self.capacity,
                size: extra,
            });
        }
        self.capacity = capacity;
    }

    fn grow_capacity(&self, needed: u64) -> Option<u64> {
        let largest = self.free.iter().map(|b| b.size).max().unwrap_or(0);
        if largest >= needed {
            return None;
        }
        let capacity = self
            .capacity
            .saturating_mul(2)
            .max(self.capacity.saturating_add(needed));
        (capacity > self.capacity).then_some(capacity)
    }
}

/// Vertex-buffer arena for one region: free-list bookkeeping plus the GPU
/// buffer. Growth allocates a larger buffer and copies the old contents
/// (allocation offsets stay stable).
pub struct GpuArena {
    buffer: wgpu::Buffer,
    list: FreeList,
}

impl GpuArena {
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            buffer: create_arena_buffer(device, INITIAL_ARENA_BYTES),
            list: FreeList::new(INITIAL_ARENA_BYTES),
        }
    }

    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    pub fn alloc(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertices: &[TerrainVertex],
    ) -> Option<ArenaAllocation> {
        if vertices.is_empty() {
            return None;
        }
        let size = vertices.len() as u64 * VERTEX_STRIDE;
        let offset = match self.list.alloc(size) {
            Some(offset) => offset,
            None => {
                self.grow(device, queue, size);
                self.list.alloc(size)?
            }
        };
        queue.write_buffer(&self.buffer, offset, bytemuck::cast_slice(vertices));
        Some(ArenaAllocation {
            vertex_offset: (offset / VERTEX_STRIDE) as u32,
            vertex_count: vertices.len() as u32,
        })
    }

    fn grow(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, needed: u64) {
        let Some(capacity) = self.list.grow_capacity(needed) else {
            return;
        };
        let new_buffer = create_arena_buffer(device, capacity);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("arena-grow"),
        });
        encoder.copy_buffer_to_buffer(&self.buffer, 0, &new_buffer, 0, self.list.capacity);
        queue.submit([encoder.finish()]);
        self.buffer = new_buffer;
        self.list.grow(capacity);
    }

    pub fn free(&mut self, allocation: ArenaAllocation) {
        self.list.free(
            allocation.vertex_offset as u64 * VERTEX_STRIDE,
            allocation.vertex_count as u64 * VERTEX_STRIDE,
        );
    }

    pub fn begin_frame(&mut self) {
        self.list.begin_frame();
    }

    pub fn used_bytes(&self) -> u64 {
        self.list.used
    }

    pub fn capacity_bytes(&self) -> u64 {
        self.list.capacity
    }
}

fn create_arena_buffer(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("region-vertices"),
        size,
        usage: wgpu::BufferUsages::VERTEX
            | wgpu::BufferUsages::COPY_DST
            | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    })
}

#[derive(Clone, Copy, Debug)]
pub struct ArenaAllocation {
    pub vertex_offset: u32,
    pub vertex_count: u32,
}

/// Facing-run metadata for one section pass inside a region arena.
#[derive(Clone, Copy, Debug, Default)]
pub struct SectionPassAlloc {
    pub base_vertex: u32,
    pub counts: [u32; FACING_COUNT],
}

impl SectionPassAlloc {
    pub fn vertex_count(&self) -> u32 {
        self.counts.iter().sum()
    }

    pub fn is_empty(&self) -> bool {
        self.vertex_count() == 0
    }

    pub fn facing_offset(&self, facing: u8) -> u32 {
        self.counts[..facing as usize].iter().sum()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SectionAlloc {
    pub passes: [SectionPassAlloc; 3],
}

/// One 8×4×8 section group: shared vertex arena + 256 local section slots.
pub struct RenderRegion {
    pub key: RegionKey,
    arena: GpuArena,
    sections: [Option<SectionAlloc>; SECTIONS_PER_REGION],
    /// Translucent per-quad sort keys (world center + normal), kept on the
    /// CPU for per-frame re-sorting as the camera moves.
    quad_sort: [Option<Box<[QuadSortData]>>; SECTIONS_PER_REGION],
    /// Per-section sorted translucent index buffers (`4q+{0,1,2,2,3,0}`
    /// following the camera-relative quad order).
    local_indices: [Option<wgpu::Buffer>; SECTIONS_PER_REGION],
}

impl RenderRegion {
    pub fn new(device: &wgpu::Device, key: RegionKey) -> Self {
        Self {
            key,
            arena: GpuArena::new(device),
            sections: [None; SECTIONS_PER_REGION],
            quad_sort: core::array::from_fn(|_| None),
            local_indices: core::array::from_fn(|_| None),
        }
    }

    pub fn update_section(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        slot: usize,
        section: &SectionMesh,
    ) {
        self.clear_section(slot);
        let mut vertices = Vec::new();
        let mut passes = [SectionPassAlloc::default(); 3];
        for (index, pass) in section.passes.iter().enumerate() {
            if pass.vertices.is_empty() {
                continue;
            }
            passes[index] = SectionPassAlloc {
                base_vertex: vertices.len() as u32,
                counts: pass.counts,
            };
            vertices.extend_from_slice(&pass.vertices);
        }
        let Some(allocation) = self.arena.alloc(device, queue, &vertices) else {
            return;
        };
        for pass in &mut passes {
            pass.base_vertex += allocation.vertex_offset;
        }
        self.sections[slot] = Some(SectionAlloc { passes });
        let translucent = TerrainPass::Translucent.index();
        if !section.passes[translucent].sort.is_empty() {
            self.quad_sort[slot] =
                Some(section.passes[translucent].sort.clone().into_boxed_slice());
        }
    }

    pub fn clear_section(&mut self, slot: usize) {
        self.quad_sort[slot] = None;
        self.local_indices[slot] = None;
        let Some(old) = self.sections[slot].take() else {
            return;
        };
        for pass in old.passes {
            if pass.is_empty() {
                continue;
            }
            self.arena.free(ArenaAllocation {
                vertex_offset: pass.base_vertex,
                vertex_count: pass.vertex_count(),
            });
        }
    }

    /// Re-sorts this section's translucent quads for `camera`
    /// (Sodium `ModelQuadNormalRelativeComparator`: ascending
    /// `(center − camera) · normal`, ties by original order) and refreshes
    /// the local index buffer. Returns `(base_vertex, index_count)` for the
    /// draw, or `None` when the section has no translucent geometry.
    pub fn sort_translucent(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        slot: usize,
        camera: [f32; 3],
    ) -> Option<(u32, u32)> {
        let sort = self.quad_sort[slot].as_ref()?;
        let pass = self.sections[slot]?.passes[TerrainPass::Translucent.index()];
        if sort.is_empty() {
            return None;
        }
        let mut order: Vec<u32> = (0..sort.len() as u32).collect();
        sort_translucent_quads(&mut order, sort, camera);
        let indices = quad_run_indices(&order);
        let needed = indices.len() as u64 * size_of::<u32>() as u64;
        let needs_new = self.local_indices[slot]
            .as_ref()
            .is_none_or(|buffer| buffer.size() < needed);
        if needs_new {
            self.local_indices[slot] = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("translucent-indices"),
                size: needed,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        let buffer = self.local_indices[slot].as_ref()?;
        queue.write_buffer(buffer, 0, bytemuck::cast_slice(&indices));
        Some((pass.base_vertex, indices.len() as u32))
    }

    pub fn translucent_index_buffer(&self, slot: usize) -> Option<&wgpu::Buffer> {
        self.local_indices[slot].as_ref()
    }

    pub fn section(&self, slot: usize) -> Option<&SectionAlloc> {
        self.sections[slot].as_ref()
    }

    pub fn begin_frame(&mut self) {
        self.arena.begin_frame();
    }

    pub fn vertex_buffer(&self) -> &wgpu::Buffer {
        self.arena.buffer()
    }

    pub fn is_empty(&self) -> bool {
        self.sections.iter().all(Option::is_none) && self.arena.used_bytes() == 0
    }

    pub fn arena_usage(&self) -> (u64, u64) {
        (self.arena.used_bytes(), self.arena.capacity_bytes())
    }
}

/// Sodium `ModelQuadNormalRelativeComparator`: sorts quad indices by
/// `(center − camera) · normal` ascending (back-to-front along each quad's
/// outward normal), ties broken by original order.
pub fn sort_translucent_quads(order: &mut [u32], sort: &[QuadSortData], camera: [f32; 3]) {
    order.sort_by(|&a, &b| {
        let key = |quad: u32| {
            let entry = &sort[quad as usize];
            (entry.center[0] - camera[0]) * entry.normal[0]
                + (entry.center[1] - camera[1]) * entry.normal[1]
                + (entry.center[2] - camera[2]) * entry.normal[2]
        };
        key(a)
            .partial_cmp(&key(b))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(&b))
    });
}

fn quad_run_indices(order: &[u32]) -> Vec<u32> {
    let mut indices = Vec::with_capacity(order.len() * 6);
    for &quad in order {
        let base = quad * 4;
        indices.extend_from_slice(&[base, base + 1, base + 2, base + 2, base + 3, base]);
    }
    indices
}

pub type RegionMap = HashMap<RegionKey, RenderRegion>;

/// Global shared quad index buffer: pattern `4q+{0,1,2, 2,3,0}` per quad.
/// Draws pass `base_vertex` pointing at their run in a region arena and
/// always start at `first_index = 0`, so the buffer only needs to cover the
/// largest single draw.
pub struct SharedQuadIndexBuffer {
    buffer: wgpu::Buffer,
    capacity_quads: u32,
}

impl SharedQuadIndexBuffer {
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            buffer: create_index_buffer(device, INITIAL_QUADS),
            capacity_quads: INITIAL_QUADS,
        }
    }

    pub fn ensure_quads(&mut self, device: &wgpu::Device, quads: u32) {
        if quads <= self.capacity_quads {
            return;
        }
        let mut capacity = self.capacity_quads;
        while capacity < quads {
            capacity = capacity.saturating_mul(2);
        }
        self.buffer = create_index_buffer(device, capacity);
        self.capacity_quads = capacity;
    }

    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }
}

fn create_index_buffer(device: &wgpu::Device, quads: u32) -> wgpu::Buffer {
    let mut indices = Vec::with_capacity(quads as usize * 6);
    for quad in 0..quads {
        let base = quad * 4;
        indices.extend_from_slice(&[base, base + 1, base + 2, base + 2, base + 3, base]);
    }
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("shared-quad-indices"),
        contents: bytemuck::cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_key_groups_eight_by_four_by_eight() {
        assert_eq!(region_key(0, 0, 0), (0, 0, 0));
        assert_eq!(region_key(7, 3, 7), (0, 0, 0));
        assert_eq!(region_key(8, 4, 8), (1, 1, 1));
        assert_eq!(region_key(-1, -1, -1), (-1, -1, -1));
    }

    #[test]
    fn local_index_matches_sodium_bit_layout() {
        assert_eq!(local_section_index(0, 0, 0), 0);
        assert_eq!(local_section_index(1, 0, 0), 1);
        assert_eq!(local_section_index(0, 0, 1), 8);
        assert_eq!(local_section_index(0, 1, 0), 64);
        assert_eq!(local_section_index(7, 3, 7), 255);
    }

    #[test]
    fn slot_and_origin_for_chunk_sections() {
        let (key, slot) = chunk_section_slot([0, 0], 0);
        assert_eq!(key, (0, -1, 0));
        assert_eq!(slot, local_section_index(0, -4, 0));
        assert_eq!(section_world_origin([32, -16], 4), [32, 0, -16]);
    }

    #[test]
    fn free_list_allocates_and_frees() {
        let mut list = FreeList::new(1024);
        let a = list.alloc(100).unwrap();
        let b = list.alloc(200).unwrap();
        assert_eq!((a, b), (0, 100));
        assert_eq!(list.used, 300);
        list.free(a, 100);
        assert_eq!(list.used, 200);
        list.begin_frame();
        list.begin_frame();
        list.begin_frame();
        let c = list.alloc(100).unwrap();
        assert_eq!(c, 0);
    }

    #[test]
    fn free_list_best_fit_picks_smallest_hole() {
        let mut list = FreeList::new(1024);
        let a = list.alloc(100).unwrap();
        let b = list.alloc(300).unwrap();
        let c = list.alloc(100).unwrap();
        let d = list.alloc(300).unwrap();
        list.free(b, 300);
        list.free(d, 300);
        for _ in 0..3 {
            list.begin_frame();
        }
        let e = list.alloc(80).unwrap();
        assert_eq!(e, b);
        assert_eq!(list.capacity, 1024);
        let _ = (a, c);
    }

    #[test]
    fn free_list_coalesces_adjacent_blocks() {
        let mut list = FreeList::new(1024);
        let a = list.alloc(100).unwrap();
        let b = list.alloc(100).unwrap();
        let c = list.alloc(100).unwrap();
        list.free(a, 100);
        list.free(c, 100);
        list.free(b, 100);
        for _ in 0..3 {
            list.begin_frame();
        }
        assert_eq!(list.free.len(), 1);
        assert_eq!(
            list.free[0],
            FreeBlock {
                offset: 0,
                size: 1024
            }
        );
        assert_eq!(list.alloc(1024), Some(0));
    }

    #[test]
    fn free_list_grows_into_new_capacity() {
        let mut list = FreeList::new(256);
        let a = list.alloc(256).unwrap();
        assert_eq!(a, 0);
        assert!(list.alloc(128).is_none());
        list.grow(512);
        assert_eq!(list.capacity, 512);
        let b = list.alloc(256).unwrap();
        assert_eq!(b, 256);
    }

    #[test]
    fn pending_frees_wait_out_the_lag() {
        let mut list = FreeList::new(256);
        let a = list.alloc(256).unwrap();
        list.free(a, 256);
        assert_eq!(list.used, 0);
        list.begin_frame();
        assert!(list.alloc(256).is_none(), "free must stay lagged");
        list.begin_frame();
        list.begin_frame();
        assert_eq!(list.alloc(256), Some(0));
    }

    #[test]
    fn lagged_free_does_not_satisfy_growth_by_aggregate() {
        let mut list = FreeList::new(256);
        let _a = list.alloc(100).unwrap();
        let b = list.alloc(100).unwrap();
        let _c = list.alloc(56).unwrap();
        list.free(b, 100);
        assert!(list.alloc(100).is_none());
        assert!(
            list.capacity - list.used >= 100,
            "aggregate free bytes overstate what best-fit can serve"
        );
        let capacity = list
            .grow_capacity(100)
            .expect("lagged free must still trigger growth");
        list.grow(capacity);
        assert!(list.alloc(100).is_some());
    }

    #[test]
    fn fragmented_free_list_grows_for_contiguous_need() {
        let mut list = FreeList::new(256);
        let a = list.alloc(64).unwrap();
        let _b = list.alloc(64).unwrap();
        let c = list.alloc(64).unwrap();
        let _d = list.alloc(64).unwrap();
        list.free(a, 64);
        list.free(c, 64);
        for _ in 0..3 {
            list.begin_frame();
        }
        assert!(list.alloc(128).is_none());
        assert!(list.capacity - list.used >= 128);
        let capacity = list
            .grow_capacity(128)
            .expect("fragmentation must still trigger growth");
        list.grow(capacity);
        assert!(list.alloc(128).is_some());
    }

    #[test]
    fn quad_index_pattern_is_deterministic() {
        let indices: Vec<u32> = (0..2u32)
            .flat_map(|q| {
                let b = q * 4;
                [b, b + 1, b + 2, b + 2, b + 3, b]
            })
            .collect();
        assert_eq!(indices, [0, 1, 2, 2, 3, 0, 4, 5, 6, 6, 7, 4]);
    }

    #[test]
    fn visible_faces_from_inside_section_sees_everything() {
        let mask = visible_faces([8.0, -56.0, 8.0], [0, -64, 0]);
        assert_eq!(mask, 0b111_1111);
    }

    #[test]
    fn visible_faces_from_far_side_hides_opposite() {
        let mask = visible_faces([1000.0, 0.0, 0.0], [0, 0, 0]);
        assert!(mask & (1 << FACING_POS_X) != 0);
        assert_eq!(mask & (1 << FACING_NEG_X), 0);
        assert!(mask & (1 << FACING_POS_Y) != 0);
        assert!(mask & (1 << FACING_NEG_Y) != 0);
    }

    #[test]
    fn visible_faces_epsilon_covers_overhang() {
        let just_inside = visible_faces([-2.9, 0.0, 0.0], [0, 0, 0]);
        assert!(just_inside & (1 << FACING_POS_X) != 0);
        let at_boundary = visible_faces([-3.0, 0.0, 0.0], [0, 0, 0]);
        assert_eq!(at_boundary & (1 << FACING_POS_X), 0);
    }

    #[test]
    fn merge_runs_combines_consecutive_and_splits_gaps() {
        let counts = [4, 8, 0, 12, 8, 0, 4];
        assert_eq!(merge_visible_runs(&counts, 100, 0b11), vec![(100, 12)]);
        assert_eq!(
            merge_visible_runs(&counts, 100, 0b1_0011),
            vec![(100, 12), (124, 8)]
        );
        assert_eq!(merge_visible_runs(&counts, 100, 0b1), vec![(100, 4)]);
        assert_eq!(
            merge_visible_runs(&counts, 100, 0b111_1111),
            vec![(100, 12), (112, 20), (132, 4)]
        );
    }

    #[test]
    fn slice_mask_counts_nonempty_facings() {
        let counts = [4, 0, 0, 12, 0, 0, 4];
        assert_eq!(slice_mask(&counts), 0b100_1001);
    }

    #[test]
    fn translucent_sort_is_back_to_front_along_normals() {
        let sort = [
            QuadSortData {
                center: [0.0, 0.0, 0.0],
                normal: [0.0, 1.0, 0.0],
            },
            QuadSortData {
                center: [0.0, -10.0, 0.0],
                normal: [0.0, 1.0, 0.0],
            },
            QuadSortData {
                center: [0.0, -5.0, 0.0],
                normal: [0.0, 1.0, 0.0],
            },
        ];
        let mut order = [0u32, 1, 2];
        sort_translucent_quads(&mut order, &sort, [0.0, 10.0, 0.0]);
        assert_eq!(order, [1, 2, 0], "deeper water draws first");
    }

    #[test]
    fn quad_run_indices_follow_permutation() {
        assert_eq!(
            quad_run_indices(&[2, 0]),
            vec![8, 9, 10, 10, 11, 8, 0, 1, 2, 2, 3, 0]
        );
    }
}
