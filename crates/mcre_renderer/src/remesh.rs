use std::sync::{
    Arc,
    mpsc::{Receiver, Sender, channel},
};

use mcre_resource_pack::ResourcePackData;
use mcre_world::{CHUNK_WIDTH, Chunk, ChunkLight, ChunkPos, World};

use crate::atlas::Atlas;
use crate::mesh::{LightMode, Mesh, MeshOptions, mesh_chunk};

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
    job.world.compute_light();
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
        chunks.push(MeshedChunk {
            pos: *pos,
            origin,
            light: chunk.light().clone(),
            mesh,
            chunk: if streamed.contains(&(pos.x, pos.z)) {
                Some(chunk.clone())
            } else {
                None
            },
        });
    }
    let mesh_ms = mesh_start.elapsed().as_secs_f64() * 1000.0;
    eprintln!(
        "remesh: generation {} load={load_ms:.1}ms light={light_ms:.1}ms mesh={mesh_ms:.1}ms chunks={}",
        job.generation,
        chunks.len(),
    );
    MeshResult {
        generation: job.generation,
        light_edits: job.light_edits,
        chunks,
    }
}

pub struct MeshWorker {
    sender: Sender<MeshJob>,
    receiver: Receiver<MeshResult>,
    next_generation: u64,
}

impl MeshWorker {
    pub fn new(
        data: Arc<ResourcePackData>,
        atlas: Arc<Atlas>,
        source: Option<StreamSource>,
    ) -> Self {
        let (job_sender, job_receiver) = channel::<MeshJob>();
        let (result_sender, result_receiver) = channel::<MeshResult>();
        std::thread::spawn(move || {
            while let Ok(job) = job_receiver.recv() {
                let result = run_job(job, &data, &atlas, source.as_ref());
                let _ = result_sender.send(result);
            }
        });
        Self {
            sender: job_sender,
            receiver: result_receiver,
            next_generation: 0,
        }
    }

    pub fn submit(
        &mut self,
        world: World,
        stream: Vec<(i32, i32)>,
        dirty: Vec<ChunkPos>,
        light_edits: Vec<(i32, i32, i32)>,
        mode: LightMode,
        options: MeshOptions,
    ) -> u64 {
        self.next_generation += 1;
        let generation = self.next_generation;
        let _ = self.sender.send(MeshJob {
            world,
            stream,
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
            entries: vec![],
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
            regions: vec![],
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
            Vec::new(),
            Vec::new(),
            LightMode::Smooth,
            MeshOptions::default(),
        );
        let second = worker.submit(
            world,
            Vec::new(),
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
