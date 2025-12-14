use bevy::prelude::*;
use noise::{NoiseFn, Perlin, Seedable};
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;

#[derive(Resource)]
pub struct SeedRng(ChaCha8Rng);

impl SeedRng {
    pub fn generate(&mut self) -> u32 {
        self.0.next_u32()
    }
}

impl Default for SeedRng {
    fn default() -> Self {
        Self(ChaCha8Rng::from_os_rng())
    }
}

#[derive(Resource)]
pub struct ChunkRng(Perlin);

impl ChunkRng {
    pub fn new(seed: u32) -> Self {
        ChunkRng(Perlin::new(seed))
    }

    pub fn seed(&self) -> u32 {
        self.0.seed()
    }

    #[allow(unused)]
    pub fn smooth_noise(&self, v: Vec2) -> f64 {
        let v = v * 0.007;
        -30. + self.0.get([v.x as f64, v.y as f64]) * 20.
    }

    pub fn fractal_noise(&self, v: Vec2, mut settings: FractalNoiseSettings) -> f64 {
        let mut sum = 0.;
        for i in 0..settings.octaves {
            sum += settings.amplitude
                * self
                    .0
                    .get([v.x as f64 * settings.freq, v.y as f64 * settings.freq]);
            settings.freq *= 2_f64.powi(i as i32 + 1);
            settings.amplitude *= settings.persistence.powi(i as i32 + 1);
        }
        sum
    }
}

#[derive(Clone, Copy)]
pub struct FractalNoiseSettings {
    pub freq: f64,
    pub amplitude: f64,
    pub octaves: u8,
    pub persistence: f64,
}
