use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::rcon::RconClient;

pub const TILE_CHUNKS: i32 = 16;
const POLL_INTERVAL: Duration = Duration::from_millis(1500);
const SAVE_INTERVAL: Duration = Duration::from_secs(60);
const NO_PROGRESS_TIMEOUT: Duration = Duration::from_secs(600);
const RATE_WINDOW: Duration = Duration::from_secs(60);
const VERIFY_ITERATIONS: usize = 3;
const VERIFY_WAIT: Duration = Duration::from_secs(300);
const PRESCAN_THREADS: usize = 8;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("RCON error: {0}")]
    Rcon(#[from] crate::rcon::Error),
    #[error("no progress for {0:?} — server appears stuck")]
    NoProgress(Duration),
    #[error("server died during generation")]
    ServerDied,
    #[error("{missing} chunks still missing after {VERIFY_ITERATIONS} verification passes")]
    VerificationFailed { missing: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkRect {
    pub min_cx: i32,
    pub min_cz: i32,
    pub max_cx: i32,
    pub max_cz: i32,
}

impl ChunkRect {
    pub fn from_radius(radius: u32) -> Self {
        let radius = radius.min(32768) as i32;
        Self {
            min_cx: -radius,
            min_cz: -radius,
            max_cx: radius - 1,
            max_cz: radius - 1,
        }
    }

    pub fn total_chunks(&self) -> u64 {
        let dx = (self.max_cx - self.min_cx + 1) as u64;
        let dz = (self.max_cz - self.min_cz + 1) as u64;
        dx * dz
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile {
    pub min_cx: i32,
    pub min_cz: i32,
    pub max_cx: i32,
    pub max_cz: i32,
}

impl Tile {
    pub fn forceload_command(&self) -> String {
        format!(
            "forceload add {} {} {} {}",
            self.min_cx * 16,
            self.min_cz * 16,
            self.max_cx * 16 + 15,
            self.max_cz * 16 + 15,
        )
    }

    pub fn unforceload_command(&self) -> String {
        format!(
            "forceload remove {} {} {} {}",
            self.min_cx * 16,
            self.min_cz * 16,
            self.max_cx * 16 + 15,
            self.max_cz * 16 + 15,
        )
    }

    pub fn contains(&self, cx: i32, cz: i32) -> bool {
        cx >= self.min_cx && cx <= self.max_cx && cz >= self.min_cz && cz <= self.max_cz
    }

    pub fn chunks(&self) -> impl Iterator<Item = (i32, i32)> {
        let (min_cx, min_cz, max_cx, max_cz) = (self.min_cx, self.min_cz, self.max_cx, self.max_cz);
        (min_cz..=max_cz).flat_map(move |cz| (min_cx..=max_cx).map(move |cx| (cx, cz)))
    }
}

pub fn tiles_for(rect: &ChunkRect) -> Vec<Tile> {
    let mut tiles = Vec::new();
    let mut z = rect.min_cz;
    while z <= rect.max_cz {
        let mut x = rect.min_cx;
        while x <= rect.max_cx {
            tiles.push(Tile {
                min_cx: x,
                min_cz: z,
                max_cx: (x + TILE_CHUNKS - 1).min(rect.max_cx),
                max_cz: (z + TILE_CHUNKS - 1).min(rect.max_cz),
            });
            x += TILE_CHUNKS;
        }
        z += TILE_CHUNKS;
    }
    tiles
}

pub fn resolve_region_dir(world: &Path) -> Option<PathBuf> {
    let modern = world
        .join("dimensions")
        .join("minecraft")
        .join("overworld")
        .join("region");
    let legacy = world.join("region");
    if modern.is_dir() {
        Some(modern)
    } else if legacy.is_dir() {
        Some(legacy)
    } else {
        None
    }
}

pub struct PrescanResult {
    pub tile_full: Vec<u64>,
    pub done_tiles: Vec<usize>,
}

impl PrescanResult {
    pub fn full_chunks(&self) -> u64 {
        self.tile_full.iter().sum()
    }
}

pub fn prescan(world: &Path, tiles: &[Tile]) -> PrescanResult {
    let Some(region_dir) = resolve_region_dir(world) else {
        return PrescanResult {
            tile_full: vec![0; tiles.len()],
            done_tiles: Vec::new(),
        };
    };
    let tile_full = chunked_map(tiles.len(), PRESCAN_THREADS, |index| {
        tile_full_chunks(&region_dir, &tiles[index])
    });
    let mut done_tiles = Vec::new();
    for (index, full) in tile_full.iter().enumerate() {
        let tile = tiles[index];
        let total = (tile.max_cx - tile.min_cx + 1) as u64 * (tile.max_cz - tile.min_cz + 1) as u64;
        if *full == total {
            done_tiles.push(index);
        }
    }
    PrescanResult {
        tile_full,
        done_tiles,
    }
}

fn tile_full_chunks(region_dir: &Path, tile: &Tile) -> u64 {
    tile.chunks()
        .filter(|&(cx, cz)| {
            mcre_anvil::chunk_status(region_dir, cx, cz)
                .map(|status| status.as_deref() == Some("minecraft:full"))
                .unwrap_or(false)
        })
        .count() as u64
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TileStatus {
    Queued,
    Active,
    Done,
}

pub struct Progress {
    pub done_chunks: u64,
    pub total_chunks: u64,
    pub active_tiles: usize,
    pub rate: f64,
    pub eta_secs: Option<u64>,
}

impl Progress {
    pub fn eta_text(&self) -> String {
        match self.eta_secs {
            None => "estimating...".to_string(),
            Some(secs) => {
                let secs = secs.max(1);
                let hours = secs / 3600;
                let minutes = (secs % 3600) / 60;
                if hours > 0 {
                    format!("{hours}h {minutes}m")
                } else if minutes > 0 {
                    format!("{minutes}m {}s", secs % 60)
                } else {
                    format!("{secs}s")
                }
            }
        }
    }
}

pub struct Summary {
    pub total_chunks: u64,
    pub elapsed: Duration,
}

pub fn run_pregen(
    world: &Path,
    rect: &ChunkRect,
    tiles: &[Tile],
    client: &mut RconClient,
    window: usize,
    is_aborted: &dyn Fn() -> bool,
    mut on_progress: impl FnMut(&Progress),
) -> Result<Summary, Error> {
    let started = Instant::now();
    let total_chunks = rect.total_chunks();
    let prescan = prescan(world, tiles);

    let mut statuses = vec![TileStatus::Queued; tiles.len()];
    let mut tile_full = prescan.tile_full;
    for index in prescan.done_tiles {
        statuses[index] = TileStatus::Done;
    }
    let mut done_chunks = tile_full.iter().sum();
    let mut rate_samples: VecDeque<(Instant, u64)> = VecDeque::from([(started, done_chunks)]);
    let mut last_progress = Instant::now();
    let mut last_save = Instant::now();

    while done_chunks < total_chunks {
        if is_aborted() {
            return Err(Error::ServerDied);
        }
        if last_progress.elapsed() >= NO_PROGRESS_TIMEOUT {
            return Err(Error::NoProgress(NO_PROGRESS_TIMEOUT));
        }

        let active = statuses
            .iter()
            .filter(|&&s| s == TileStatus::Active)
            .count();
        let mut available = window.saturating_sub(active);
        for index in 0..tiles.len() {
            if available == 0 {
                break;
            }
            if statuses[index] == TileStatus::Queued {
                client.run(&tiles[index].forceload_command())?;
                statuses[index] = TileStatus::Active;
                available -= 1;
            }
        }

        if let Some(region_dir) = resolve_region_dir(world) {
            for index in 0..tiles.len() {
                if statuses[index] != TileStatus::Active {
                    continue;
                }
                let tile = tiles[index];
                let full = tile_full_chunks(&region_dir, &tile);
                if full > tile_full[index] {
                    done_chunks += full - tile_full[index];
                    tile_full[index] = full;
                    last_progress = Instant::now();
                    rate_samples.push_back((Instant::now(), done_chunks));
                }
                let total =
                    (tile.max_cx - tile.min_cx + 1) as u64 * (tile.max_cz - tile.min_cz + 1) as u64;
                if full >= total {
                    statuses[index] = TileStatus::Done;
                    client.run(&tile.unforceload_command())?;
                }
            }
        }

        while let Some((when, _)) = rate_samples.front()
            && when.elapsed() > RATE_WINDOW
            && rate_samples.len() > 1
        {
            rate_samples.pop_front();
        }
        let rate = rate(&rate_samples);
        let eta_secs = eta(rate, total_chunks, done_chunks);

        on_progress(&Progress {
            done_chunks,
            total_chunks,
            active_tiles: statuses
                .iter()
                .filter(|&&s| s == TileStatus::Active)
                .count(),
            rate,
            eta_secs,
        });

        if last_save.elapsed() >= SAVE_INTERVAL {
            client.run_with_deadline("save-all flush", Duration::from_secs(600))?;
            last_save = Instant::now();
        }

        std::thread::sleep(POLL_INTERVAL);
    }

    Ok(Summary {
        total_chunks,
        elapsed: started.elapsed(),
    })
}

pub struct VerifySummary {
    pub checked_chunks: u64,
    pub elapsed: Duration,
}

pub fn verify(
    world: &Path,
    tiles: &[Tile],
    client: &mut RconClient,
    is_aborted: &dyn Fn() -> bool,
) -> Result<VerifySummary, Error> {
    let started = Instant::now();
    let chunks: Vec<(i32, i32)> = tiles.iter().flat_map(|tile| tile.chunks()).collect();
    let mut missing = sweep_missing(world, &chunks);

    for _ in 0..VERIFY_ITERATIONS {
        if missing.is_empty() {
            break;
        }
        if is_aborted() {
            return Err(Error::ServerDied);
        }

        let mut retried = Vec::new();
        for tile in tiles {
            let touches_missing = missing.iter().any(|&(cx, cz)| tile.contains(cx, cz));
            if touches_missing {
                client.run(&tile.forceload_command())?;
                retried.push(*tile);
            }
        }

        let deadline = Instant::now() + VERIFY_WAIT;
        while !missing.is_empty() && Instant::now() < deadline {
            if is_aborted() {
                return Err(Error::ServerDied);
            }
            std::thread::sleep(POLL_INTERVAL);
            let region_dir = resolve_region_dir(world);
            if let Some(region_dir) = &region_dir {
                missing.retain(|&(cx, cz)| {
                    mcre_anvil::chunk_status(region_dir, cx, cz)
                        .map(|status| status.as_deref() != Some("minecraft:full"))
                        .unwrap_or(true)
                });
            }
        }

        for tile in retried {
            client.run(&tile.unforceload_command())?;
        }
        missing = sweep_missing(world, &chunks);
    }

    if !missing.is_empty() {
        return Err(Error::VerificationFailed {
            missing: missing.len(),
        });
    }

    Ok(VerifySummary {
        checked_chunks: chunks.len() as u64,
        elapsed: started.elapsed(),
    })
}

fn sweep_missing(world: &Path, chunks: &[(i32, i32)]) -> Vec<(i32, i32)> {
    let Some(region_dir) = resolve_region_dir(world) else {
        return chunks.to_vec();
    };
    chunked_map(chunks.len(), PRESCAN_THREADS, |index| {
        let (cx, cz) = chunks[index];
        let present = mcre_anvil::chunk_present(&region_dir, cx, cz).unwrap_or(false);
        (cx, cz, present)
    })
    .into_iter()
    .filter_map(|(cx, cz, present)| if present { None } else { Some((cx, cz)) })
    .collect()
}

fn chunked_map<T: Send>(items: usize, threads: usize, f: impl Fn(usize) -> T + Sync) -> Vec<T> {
    let threads = threads.max(1).min(items.max(1));
    let chunk = items.div_ceil(threads);
    let f = &f;
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|worker| {
                let start = worker * chunk;
                let end = (start + chunk).min(items);
                scope.spawn(move || (start..end).map(f).collect::<Vec<T>>())
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().unwrap_or_default())
            .collect()
    })
}

fn rate(samples: &VecDeque<(Instant, u64)>) -> f64 {
    let (first, last) = match (samples.front(), samples.back()) {
        (Some(first), Some(last)) => (first, last),
        _ => return 0.0,
    };
    let elapsed = last.0.saturating_duration_since(first.0).as_secs_f64();
    if elapsed <= 0.0 {
        return 0.0;
    }
    (last.1.saturating_sub(first.1)) as f64 / elapsed
}

fn eta(rate: f64, total: u64, done: u64) -> Option<u64> {
    if rate <= 0.0 || done >= total {
        return None;
    }
    Some(((total - done) as f64 / rate) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radius_maps_to_centered_rect() {
        let rect = ChunkRect::from_radius(512);
        assert_eq!(rect.min_cx, -512);
        assert_eq!(rect.max_cx, 511);
        assert_eq!(rect.min_cz, -512);
        assert_eq!(rect.max_cz, 511);
        assert_eq!(rect.total_chunks(), 1024 * 1024);
    }

    #[test]
    fn tiles_cover_rect_with_clipped_edges() {
        let rect = ChunkRect::from_radius(20);
        let tiles = tiles_for(&rect);
        assert_eq!(rect.total_chunks(), 40 * 40);
        let covered: u64 = tiles
            .iter()
            .map(|t| (t.max_cx - t.min_cx + 1) as u64 * (t.max_cz - t.min_cz + 1) as u64)
            .sum();
        assert_eq!(covered, rect.total_chunks());
        assert!(tiles.iter().all(|t| {
            t.min_cx >= rect.min_cx
                && t.max_cx <= rect.max_cx
                && t.min_cz >= rect.min_cz
                && t.max_cz <= rect.max_cz
        }));
    }

    #[test]
    fn tiles_have_no_overlap() {
        let rect = ChunkRect::from_radius(20);
        let tiles = tiles_for(&rect);
        let mut seen = std::collections::HashSet::new();
        for tile in &tiles {
            for chunk in tile.chunks() {
                assert!(seen.insert(chunk));
            }
        }
    }

    #[test]
    fn forceload_uses_block_coordinates() {
        let tile = Tile {
            min_cx: 16,
            min_cz: -32,
            max_cx: 31,
            max_cz: -17,
        };
        assert_eq!(tile.forceload_command(), "forceload add 256 -512 511 -257");
        assert_eq!(
            tile.unforceload_command(),
            "forceload remove 256 -512 511 -257"
        );
    }

    #[test]
    fn rate_and_eta_math() {
        let now = Instant::now();
        let samples = VecDeque::from([
            (now, 0u64),
            (now + Duration::from_secs(10), 50),
            (now + Duration::from_secs(20), 100),
        ]);
        let rate = rate(&samples);
        assert!((rate - 5.0).abs() < 0.001);
        assert_eq!(eta(rate, 200, 100), Some(20));
        assert_eq!(eta(0.0, 200, 100), None);
    }

    #[test]
    fn prescan_finds_full_tiles_on_disk() {
        let dir = std::env::temp_dir().join(format!("mcre_gen_pregen_{}", std::process::id()));
        let world = dir.join("world");
        let region = resolve_region_dir(&world).map(|_| ());
        assert_eq!(region, None);

        let region_dir = world
            .join("dimensions")
            .join("minecraft")
            .join("overworld")
            .join("region");
        std::fs::create_dir_all(&region_dir).unwrap();
        write_full_chunk(&region_dir, 0, 0, "minecraft:full");
        write_full_chunk(&region_dir, 1, 0, "minecraft:features");

        let tiles = tiles_for(&ChunkRect {
            min_cx: 0,
            min_cz: 0,
            max_cx: 15,
            max_cz: 15,
        });
        let result = prescan(&world, &tiles);
        assert_eq!(result.full_chunks(), 1);
        assert!(result.done_tiles.is_empty());

        let tiles = tiles_for(&ChunkRect {
            min_cx: 0,
            min_cz: 0,
            max_cx: 0,
            max_cz: 0,
        });
        let result = prescan(&world, &tiles);
        assert_eq!(result.full_chunks(), 1);
        assert_eq!(result.done_tiles, vec![0]);

        std::fs::remove_dir_all(&dir).ok();
    }

    fn write_full_chunk(region_dir: &Path, cx: i32, cz: i32, status: &str) {
        let path = mcre_anvil::region_file_path(region_dir, cx, cz);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();

        let mut nbt = vec![0x0a, 0x00, 0x00];
        let key = b"status";
        nbt.extend_from_slice(&[0x08, 0x00, key.len() as u8]);
        nbt.extend_from_slice(key);
        nbt.extend_from_slice(&(status.len() as u16).to_be_bytes());
        nbt.extend_from_slice(status.as_bytes());
        nbt.push(0x00);

        let mut gzip = Vec::new();
        {
            use std::io::Write;
            let mut encoder =
                flate2::write::GzEncoder::new(&mut gzip, flate2::Compression::default());
            encoder.write_all(&nbt).unwrap();
            encoder.finish().unwrap();
        }

        let mut content = std::fs::read(&path).unwrap_or_default();
        if content.len() < 8192 {
            content.resize(8192, 0);
        }
        let sectors_used = ((content.len() - 8192) / 4096) as u32;
        let local = ((cz.rem_euclid(32)) * 32 + (cx.rem_euclid(32))) as usize;
        content[local * 4..local * 4 + 4].copy_from_slice(&sector_entry(2 + sectors_used, 1));
        let mut sector = vec![0u8; 4096];
        let body_len = (gzip.len() + 1) as u32;
        sector[0..4].copy_from_slice(&body_len.to_be_bytes());
        sector[4] = 1;
        sector[5..5 + gzip.len()].copy_from_slice(&gzip);
        content.extend_from_slice(&sector);
        std::fs::write(path, content).unwrap();
    }

    fn sector_entry(offset: u32, count: u8) -> [u8; 4] {
        let mut out = [0u8; 4];
        out[..3].copy_from_slice(&offset.to_be_bytes()[1..]);
        out[3] = count;
        out
    }
}
