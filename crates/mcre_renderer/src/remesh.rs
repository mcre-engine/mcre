use std::sync::{
    Arc,
    mpsc::{Receiver, Sender, channel},
};

use mcre_resource_pack::ResourcePackData;
use mcre_world::{CHUNK_WIDTH, Chunk, ChunkLight, ChunkPos, World};

use crate::atlas::Atlas;
use crate::mesh::{LightMode, Mesh, MeshOptions, SectionMesh, mesh_chunk, pack_chunk_sections};

/// Save-backed chunk source for streaming jobs. `None` in the worker means
/// streaming is disabled (demo mode).
#[derive(Clone)]
pub struct StreamSource {
    pub region_dir: std::path::PathBuf,
    pub blocks: Arc<mcre_anvil::BlockResolver>,
    pub biomes: Arc<mcre_anvil::BiomeResolver>,
}

pub struct MeshJob {
    pub world: World,
    pub stream: Vec<(i32, i32)>,
    pub recompute_light: bool,
    pub dirty: Vec<ChunkPos>,
    pub light_edits: Vec<(i32, i32, i32)>,
    pub generation: u64,
    pub mode: LightMode,
    pub options: MeshOptions,
}

pub struct MeshedChunk {
    pub pos: ChunkPos,
    pub origin: [i32; 2],
    pub light: ChunkLight,
    pub mesh: Mesh,
    /// Packed per-section GPU-ready geometry (24 sections × 3 passes).
    pub sections: [Option<SectionMesh>; mcre_world::SECTIONS_PER_CHUNK],
    /// Block data for streamed-in chunks, which the main world lacks.
    /// `None` for ordinary remeshes (main world already has the blocks).
    pub chunk: Option<Chunk>,
}

pub struct MeshResult {
    pub generation: u64,
    pub light_edits: Vec<(i32, i32, i32)>,
    pub chunks: Vec<MeshedChunk>,
}

pub fn run_job(
    mut job: MeshJob,
    data: &ResourcePackData,
    atlas: &Atlas,
    source: Option<&StreamSource>,
) -> MeshResult {
    let streaming = !job.stream.is_empty();
    let load_start = std::time::Instant::now();
    let mut streamed: Vec<(i32, i32)> = Vec::new();
    if !job.stream.is_empty() {
        let mut stats = mcre_anvil::ImportStats::default();
        for &(cx, cz) in &job.stream {
            let chunk = match source {
                Some(source) => match mcre_anvil::load_chunk(
                    &source.region_dir,
                    cx,
                    cz,
                    &source.blocks,
                    &source.biomes,
                    &mut stats,
                ) {
                    Ok(chunk) => chunk,
                    Err(err) => {
                        eprintln!("remesh: skipping corrupt chunk ({cx}, {cz}): {err}");
                        None
                    }
                },
                None => None,
            };
            let chunk = chunk.unwrap_or_else(|| Chunk::new_at(mcre_world::ChunkPos::new(cx, cz)));
            job.world.replace_chunk(chunk);
            streamed.push((cx, cz));
        }
    }
    let load_ms = load_start.elapsed().as_secs_f64() * 1000.0;
    let light_start = std::time::Instant::now();
    if job.recompute_light {
        job.world.mark_light_full();
    }
    job.world.compute_light();
    if job.recompute_light {
        job.world.mark_all_dirty();
    }
    let light_ms = light_start.elapsed().as_secs_f64() * 1000.0;
    let mesh_start = std::time::Instant::now();
    // Main-side marks ride along explicitly (the main world drains its own
    // list before cloning); stream replaces mark the snapshot directly.
    let mut dirty = job.dirty;
    for pos in job.world.take_dirty() {
        if !dirty.contains(&pos) {
            dirty.push(pos);
        }
    }
    let mut chunks = Vec::with_capacity(dirty.len());
    for pos in &dirty {
        let Some(chunk) = job.world.iter_chunks().find(|c| c.pos() == *pos) else {
            continue;
        };
        let origin = [pos.x * CHUNK_WIDTH, pos.z * CHUNK_WIDTH];
        let mesh = mesh_chunk(
            data,
            atlas,
            chunk,
            &job.world,
            origin,
            job.mode,
            job.options,
        );
        let sections = pack_chunk_sections(&mesh, origin);
        chunks.push(MeshedChunk {
            pos: *pos,
            origin,
            light: chunk.light().clone(),
            mesh,
            sections,
            chunk: if streamed.contains(&(pos.x, pos.z)) {
                Some(chunk.clone())
            } else {
                None
            },
        });
    }
    let mesh_ms = mesh_start.elapsed().as_secs_f64() * 1000.0;
    if !streaming {
        eprintln!(
            "remesh: generation {} load={load_ms:.1}ms light={light_ms:.1}ms mesh={mesh_ms:.1}ms chunks={}",
            job.generation,
            chunks.len(),
        );
    }
    MeshResult {
        generation: job.generation,
        light_edits: job.light_edits,
        chunks,
    }
}

pub struct MeshWorker {
    background_sender: Sender<MeshJob>,
    priority_sender: Sender<MeshJob>,
    receiver: Receiver<MeshResult>,
    next_generation: u64,
}

impl MeshWorker {
    pub fn new(
        data: Arc<ResourcePackData>,
        atlas: Arc<Atlas>,
        source: Option<StreamSource>,
    ) -> Self {
        let (background_sender, background_receiver) = channel::<MeshJob>();
        let (priority_sender, priority_receiver) = channel::<MeshJob>();
        let (result_sender, result_receiver) = channel::<MeshResult>();
        let priority_data = data.clone();
        let priority_atlas = atlas.clone();
        let priority_source = source.clone();
        let priority_result_sender = result_sender.clone();
        std::thread::spawn(move || {
            while let Ok(job) = background_receiver.recv() {
                let result = run_job(job, &data, &atlas, source.as_ref());
                let _ = result_sender.send(result);
            }
        });
        std::thread::spawn(move || {
            while let Ok(job) = priority_receiver.recv() {
                let (job, superseded) = newest_queued_job(job, &priority_receiver);
                for generation in superseded {
                    let _ = priority_result_sender.send(MeshResult {
                        generation,
                        light_edits: Vec::new(),
                        chunks: Vec::new(),
                    });
                }
                let result = run_job(
                    job,
                    &priority_data,
                    &priority_atlas,
                    priority_source.as_ref(),
                );
                let _ = priority_result_sender.send(result);
            }
        });
        Self {
            background_sender,
            priority_sender,
            receiver: result_receiver,
            next_generation: 0,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn submit(
        &mut self,
        world: World,
        stream: Vec<(i32, i32)>,
        recompute_light: bool,
        dirty: Vec<ChunkPos>,
        light_edits: Vec<(i32, i32, i32)>,
        mode: LightMode,
        options: MeshOptions,
    ) -> u64 {
        let sender = self.background_sender.clone();
        self.submit_to(
            &sender,
            world,
            stream,
            recompute_light,
            dirty,
            light_edits,
            mode,
            options,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn submit_priority(
        &mut self,
        world: World,
        stream: Vec<(i32, i32)>,
        recompute_light: bool,
        dirty: Vec<ChunkPos>,
        light_edits: Vec<(i32, i32, i32)>,
        mode: LightMode,
        options: MeshOptions,
    ) -> u64 {
        let sender = self.priority_sender.clone();
        self.submit_to(
            &sender,
            world,
            stream,
            recompute_light,
            dirty,
            light_edits,
            mode,
            options,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn submit_to(
        &mut self,
        sender: &Sender<MeshJob>,
        world: World,
        stream: Vec<(i32, i32)>,
        recompute_light: bool,
        dirty: Vec<ChunkPos>,
        light_edits: Vec<(i32, i32, i32)>,
        mode: LightMode,
        options: MeshOptions,
    ) -> u64 {
        self.next_generation += 1;
        let generation = self.next_generation;
        let _ = sender.send(MeshJob {
            world,
            stream,
            recompute_light,
            dirty,
            light_edits,
            generation,
            mode,
            options,
        });
        generation
    }

    pub fn poll(&self) -> Vec<MeshResult> {
        let mut results = Vec::new();
        while let Ok(result) = self.receiver.try_recv() {
            results.push(result);
        }
        results
    }
}

fn newest_queued_job(mut job: MeshJob, receiver: &Receiver<MeshJob>) -> (MeshJob, Vec<u64>) {
    let mut superseded = Vec::new();
    while let Ok(newer) = receiver.try_recv() {
        superseded.push(job.generation);
        job = newer;
    }
    (job, superseded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcre_resource_pack::Colormaps;
    use mcre_world::Block;

    fn flat_map(rgb: [u8; 3]) -> Box<[u8]> {
        let mut v = Vec::with_capacity(65536 * 3);
        for _ in 0..65536 {
            v.extend_from_slice(&rgb);
        }
        v.into_boxed_slice()
    }

    fn test_data() -> ResourcePackData {
        ResourcePackData {
            fluid_textures: Default::default(),
            chest_textures: Default::default(),
            font_textures: Default::default(),
            entries: vec![],
            particle_texture_ids: vec![],
            texture_table: vec![],
            culling: Default::default(),
            colormaps: Colormaps {
                grass: flat_map([10, 20, 30]),
                foliage: flat_map([40, 50, 60]),
                dry_foliage: flat_map([70, 80, 90]),
            },
        }
    }

    fn test_atlas() -> Atlas {
        Atlas {
            width: 16,
            height: 16,
            pixels: vec![0; 16 * 16 * 4],
            mips: Vec::new(),
            regions: vec![],
            has_alpha: vec![],
            animations: Vec::new(),
            fallback: crate::AtlasRegion {
                x: 0,
                y: 0,
                width: 16,
                height: 16,
            },
        }
    }

    fn poll_blocking(worker: &MeshWorker) -> Vec<MeshResult> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let results = worker.poll();
            if !results.is_empty() {
                return results;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "worker produced no result"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    fn empty_job(generation: u64) -> MeshJob {
        MeshJob {
            world: World::new(0),
            stream: Vec::new(),
            recompute_light: false,
            dirty: Vec::new(),
            light_edits: Vec::new(),
            generation,
            mode: LightMode::Smooth,
            options: MeshOptions::default(),
        }
    }

    #[test]
    fn priority_queue_keeps_only_the_newest_queued_snapshot() {
        let (sender, receiver) = channel();
        assert!(sender.send(empty_job(2)).is_ok());
        assert!(sender.send(empty_job(3)).is_ok());

        let (job, superseded) = newest_queued_job(empty_job(1), &receiver);

        assert_eq!(job.generation, 3);
        assert_eq!(superseded, [1, 2]);
    }

    #[test]
    fn worker_lights_and_meshes_dirty_chunks() {
        let data = Arc::new(test_data());
        let atlas = Arc::new(test_atlas());
        let mut worker = MeshWorker::new(data, atlas, None);

        let mut world = World::new(0);
        world.compute_light();
        world.set_block((0, 0, 0), Block::STONE.default_state());
        let dirty = world.take_dirty();
        assert!(!dirty.is_empty());
        let light_edits = world.pending_light_edits();
        assert!(!light_edits.is_empty());
        let generation = worker.submit(
            world,
            Vec::new(),
            false,
            dirty,
            light_edits,
            LightMode::Smooth,
            MeshOptions::default(),
        );

        let results = poll_blocking(&worker);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].generation, generation);
        assert_eq!(results[0].chunks.len(), 1);
        for chunk in &results[0].chunks {
            assert_eq!(chunk.light.sky_level(0, 10, 0), 15);
            assert!(chunk.chunk.is_none());
        }
    }

    #[test]
    fn worker_streams_missing_chunks_as_air() {
        let data = Arc::new(test_data());
        let atlas = Arc::new(test_atlas());
        let source = StreamSource {
            region_dir: std::path::PathBuf::from("/nonexistent"),
            blocks: Arc::new(mcre_anvil::BlockResolver::new()),
            biomes: Arc::new(mcre_anvil::BiomeResolver::new()),
        };
        let mut worker = MeshWorker::new(data, atlas, Some(source));

        let world = World::new(0);
        let generation = worker.submit(
            world,
            vec![(3, -2)],
            true,
            Vec::new(),
            Vec::new(),
            LightMode::Smooth,
            MeshOptions::default(),
        );

        let results = poll_blocking(&worker);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].generation, generation);
        assert_eq!(results[0].chunks.len(), 1);
        let streamed = &results[0].chunks[0];
        assert_eq!((streamed.pos.x, streamed.pos.z), (3, -2));
        let chunk = streamed.chunk.as_ref().expect("streamed chunk data");
        assert_eq!((chunk.pos().x, chunk.pos().z), (3, -2));
    }

    #[test]
    fn generations_increase_per_submit() {
        let data = Arc::new(test_data());
        let atlas = Arc::new(test_atlas());
        let mut worker = MeshWorker::new(data, atlas, None);

        let world = World::new(0);
        let first = worker.submit(
            world.clone(),
            Vec::new(),
            false,
            Vec::new(),
            Vec::new(),
            LightMode::Smooth,
            MeshOptions::default(),
        );
        let second = worker.submit(
            world,
            Vec::new(),
            false,
            Vec::new(),
            Vec::new(),
            LightMode::Flat,
            MeshOptions::default(),
        );
        assert_eq!((first, second), (1, 2));

        let mut gens: Vec<u64> = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while gens.len() < 2 {
            gens.extend(poll_blocking(&worker).iter().map(|r| r.generation));
            assert!(
                std::time::Instant::now() < deadline,
                "worker produced no result"
            );
        }
        assert_eq!(gens, vec![1, 2]);
    }
}
