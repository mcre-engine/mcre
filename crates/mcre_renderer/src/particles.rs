use bytemuck::{Pod, Zeroable};
use mcre_world::World;

use crate::atlas::AtlasRegion;

const MAX_PARTICLES: usize = 4096;
#[cfg(test)]
const PARTICLES_PER_BLOCK: usize = 64;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ParticleVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    pub light: [f32; 2],
}

#[derive(Default)]
pub struct ParticleSystem {
    particles: Vec<TerrainParticle>,
    tick_remainder: f32,
    seed: u64,
}

struct TerrainParticle {
    position: [f32; 3],
    previous: [f32; 3],
    velocity: [f32; 3],
    size: f32,
    uv: [f32; 4],
    age: u32,
    lifetime: u32,
}

impl ParticleSystem {
    pub fn spawn_block_break(
        &mut self,
        block: [i32; 3],
        sprite: AtlasRegion,
        atlas_size: [u32; 2],
    ) {
        let seed = self.seed
            ^ (block[0] as u32 as u64).wrapping_mul(3_129_871)
            ^ (block[1] as u32 as u64).wrapping_mul(116_129_781)
            ^ (block[2] as u32 as u64).wrapping_mul(42_317_861);
        self.seed = self.seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut random = ParticleRandom::new(seed);

        for y in 0..4 {
            for z in 0..4 {
                for x in 0..4 {
                    if self.particles.len() >= MAX_PARTICLES {
                        return;
                    }
                    let relative = [
                        (x as f32 + 0.5) / 4.0,
                        (y as f32 + 0.5) / 4.0,
                        (z as f32 + 0.5) / 4.0,
                    ];
                    let mut velocity = [
                        relative[0] - 0.5 + (random.next_f32() * 2.0 - 1.0) * 0.4,
                        relative[1] - 0.5 + (random.next_f32() * 2.0 - 1.0) * 0.4,
                        relative[2] - 0.5 + (random.next_f32() * 2.0 - 1.0) * 0.4,
                    ];
                    let speed = (random.next_f32() + random.next_f32() + 1.0) * 0.15;
                    let length = (velocity[0] * velocity[0]
                        + velocity[1] * velocity[1]
                        + velocity[2] * velocity[2])
                        .sqrt();
                    if length > 0.0 {
                        for component in &mut velocity {
                            *component = *component / length * speed * 0.4;
                        }
                    }
                    velocity[1] += 0.1;

                    let lifetime = (4.0 / (random.next_f32() * 0.9 + 0.1)) as u32;
                    let size = 0.05 * (random.next_f32() + 1.0);
                    let u_offset = random.next_f32() * 3.0;
                    let v_offset = random.next_f32() * 3.0;
                    let uv = [
                        (sprite.x as f32 + (u_offset + 1.0) * sprite.width as f32 * 0.25)
                            / atlas_size[0] as f32,
                        (sprite.x as f32 + u_offset * sprite.width as f32 * 0.25)
                            / atlas_size[0] as f32,
                        (sprite.y as f32 + v_offset * sprite.height as f32 * 0.25)
                            / atlas_size[1] as f32,
                        (sprite.y as f32 + (v_offset + 1.0) * sprite.height as f32 * 0.25)
                            / atlas_size[1] as f32,
                    ];
                    let position = [
                        block[0] as f32 + relative[0],
                        block[1] as f32 + relative[1],
                        block[2] as f32 + relative[2],
                    ];
                    self.particles.push(TerrainParticle {
                        position,
                        previous: position,
                        velocity,
                        size,
                        uv,
                        age: 0,
                        lifetime,
                    });
                }
            }
        }
    }

    pub fn update(&mut self, delta_seconds: f32, world: &World) {
        self.tick_remainder += delta_seconds * 20.0;
        while self.tick_remainder >= 1.0 {
            self.tick_remainder -= 1.0;
            for particle in &mut self.particles {
                if particle.age >= particle.lifetime {
                    continue;
                }
                tick_particle(particle, world);
            }
            self.particles
                .retain(|particle| particle.age < particle.lifetime);
        }
    }

    pub fn vertices(
        &self,
        camera_position: [f32; 3],
        camera_forward: [f32; 3],
        camera_right: [f32; 3],
        world: &World,
    ) -> Vec<ParticleVertex> {
        let camera_up = cross(camera_right, camera_forward);
        let mut particles: Vec<_> = self.particles.iter().collect();
        particles.sort_by(|a, b| {
            distance_squared(b.position, camera_position)
                .total_cmp(&distance_squared(a.position, camera_position))
        });
        let mut vertices = Vec::with_capacity(particles.len() * 6);
        for particle in particles {
            let position: [f32; 3] = core::array::from_fn(|axis| {
                particle.previous[axis]
                    + (particle.position[axis] - particle.previous[axis]) * self.tick_remainder
            });
            let (block, sky) = world.light_level((
                position[0].floor() as i32,
                position[1].floor() as i32,
                position[2].floor() as i32,
            ));
            let light = [f32::from(block), f32::from(sky)];
            let [u0, u1, v0, v1] = particle.uv;
            let corners = [
                (1.0, -1.0, u1, v1),
                (1.0, 1.0, u1, v0),
                (-1.0, 1.0, u0, v0),
                (-1.0, -1.0, u0, v1),
            ];
            let points: [[f32; 3]; 4] = core::array::from_fn(|index| {
                let (x, y, _, _) = corners[index];
                core::array::from_fn(|axis| {
                    position[axis] + particle.size * (camera_right[axis] * x + camera_up[axis] * y)
                })
            });
            for index in [0, 1, 2, 0, 2, 3] {
                let (_, _, u, v) = corners[index];
                vertices.push(ParticleVertex {
                    position: points[index],
                    uv: [u, v],
                    color: [0.6, 0.6, 0.6, 1.0],
                    light,
                });
            }
        }
        vertices
    }

    pub fn len(&self) -> usize {
        self.particles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.particles.is_empty()
    }
}

fn tick_particle(particle: &mut TerrainParticle, world: &World) {
    particle.previous = particle.position;
    particle.age += 1;
    particle.velocity[1] -= 0.04;

    let old_y = particle.position[1];
    let next_y = old_y + particle.velocity[1];
    let block_y = next_y.floor() as i32;
    let block_pos = (
        particle.position[0].floor() as i32,
        block_y,
        particle.position[2].floor() as i32,
    );
    let on_ground = particle.velocity[1] < 0.0
        && old_y >= block_y as f32 + 1.0
        && world
            .get_block(block_pos)
            .is_some_and(|state| state.collision_full());

    particle.position[0] += particle.velocity[0];
    particle.position[1] = if on_ground {
        particle.velocity[1] = 0.0;
        block_y as f32 + 1.0
    } else {
        next_y
    };
    particle.position[2] += particle.velocity[2];

    for component in &mut particle.velocity {
        *component *= 0.98;
    }
    if on_ground {
        particle.velocity[0] *= 0.7;
        particle.velocity[2] *= 0.7;
    }
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn distance_squared(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

struct ParticleRandom(u64);

impl ParticleRandom {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_add(0x9E37_79B9_7F4A_7C15))
    }

    fn next_u32(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 16) as u32
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (1.0 / 16_777_216.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_break_spawns_sixty_four_textured_quads() {
        let mut particles = ParticleSystem::default();
        particles.spawn_block_break(
            [2, 3, 4],
            AtlasRegion {
                x: 16,
                y: 32,
                width: 16,
                height: 16,
            },
            [128, 128],
        );
        assert_eq!(particles.len(), PARTICLES_PER_BLOCK);

        let world = World::new(0);
        let vertices =
            particles.vertices([0.0, 0.0, 0.0], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0], &world);
        assert_eq!(vertices.len(), PARTICLES_PER_BLOCK * 6);
        assert!(vertices.iter().all(|vertex| {
            (16.0 / 128.0..=32.0 / 128.0).contains(&vertex.uv[0])
                && (32.0 / 128.0..=48.0 / 128.0).contains(&vertex.uv[1])
        }));
    }

    #[test]
    fn particles_expire_after_their_lifetime() {
        let mut particles = ParticleSystem::default();
        particles.spawn_block_break(
            [0, 2, 0],
            AtlasRegion {
                x: 0,
                y: 0,
                width: 16,
                height: 16,
            },
            [16, 16],
        );
        let world = World::new(0);
        particles.update(3.0, &world);
        assert!(particles.is_empty());
    }
}
