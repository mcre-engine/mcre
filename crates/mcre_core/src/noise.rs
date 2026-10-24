//! Vanilla worldgen noise ported from `net.minecraft` for biome tinting.
//!
//! Covers exactly what swamp grass color needs: `LegacyRandomSource` (the
//! `java.util.Random` LCG), the `GradientNoise` permutation shuffle, 2D
//! simplex evaluation, and the `BIOME_INFO_NOISE` table (seed 2345, offsets
//! discarded). Values are verified bit-for-bit against a live-JVM probe
//! (see `biome_info_noise_oracle`).

const LCG_MULTIPLIER: u64 = 25214903917;
const LCG_ADDEND: u64 = 11;
const LCG_MASK: u64 = (1 << 48) - 1;

const fn lcg_seed(seed: u64) -> u64 {
    (seed ^ LCG_MULTIPLIER) & LCG_MASK
}

const fn lcg_next(state: u64, bits: u32) -> (u64, u64) {
    let next = state.wrapping_mul(LCG_MULTIPLIER).wrapping_add(LCG_ADDEND) & LCG_MASK;
    (next, next >> (48 - bits))
}

const fn lcg_next_int(state: u64, bound: u32) -> (u64, u32) {
    if bound == 0 || bound.count_ones() == 1 {
        let (next, bits) = lcg_next(state, 31);
        return (next, ((bound as u64 * bits) >> 31) as u32);
    }
    let mut current = state;
    loop {
        let (next, bits) = lcg_next(current, 31);
        current = next;
        let value = (bits % bound as u64) as u32;
        if bits.wrapping_sub(value as u64) + (bound as u64 - 1) < (1 << 31) {
            return (current, value);
        }
    }
}

const fn lcg_next_double(mut state: u64) -> (u64, u64) {
    let high;
    let low;
    (state, high) = lcg_next(state, 26);
    (state, low) = lcg_next(state, 27);
    (state, (high << 27) + low)
}

const GRADIENT: [(i32, i32, i32); 16] = [
    (1, 1, 0),
    (-1, 1, 0),
    (1, -1, 0),
    (-1, -1, 0),
    (1, 0, 1),
    (-1, 0, 1),
    (1, 0, -1),
    (-1, 0, -1),
    (0, 1, 1),
    (0, -1, 1),
    (0, 1, -1),
    (0, -1, -1),
    (1, 1, 0),
    (0, -1, 1),
    (-1, 1, 0),
    (0, -1, -1),
];

/// The `GradientNoise` permutation table for seed 2345 with discarded
/// offsets (three `nextDouble` draws consumed, multiplied by 0.0).
const BIOME_INFO_PERMS: [u8; 256] = {
    let mut perms = [0u8; 256];
    let mut i = 0;
    while i < 256 {
        perms[i] = i as u8;
        i += 1;
    }
    let mut state = {
        // The three offset draws are discarded (scale 0.0) but still
        // advance the LCG exactly like the Java constructor.
        let (s, _) = lcg_next_double(lcg_seed(2345));
        let (s, _) = lcg_next_double(s);
        let (s, _) = lcg_next_double(s);
        s
    };
    let mut k = 0;
    while k < 256 {
        let (next, offset) = lcg_next_int(state, 256 - k as u32);
        state = next;
        let a = k;
        let b = k + offset as usize;
        let tmp = perms[a];
        perms[a] = perms[b];
        perms[b] = tmp;
        k += 1;
    }
    perms
};

const fn permute(x: i32) -> usize {
    BIOME_INFO_PERMS[(x & 0xFF) as usize] as usize
}

fn corner_noise(index: usize, x: f64, y: f64) -> f64 {
    let mut t = 0.5 - x * x - y * y;
    if t < 0.0 {
        return 0.0;
    }
    t *= t;
    let (gx, gy, _) = GRADIENT[index];
    t * t * (gx as f64 * x + gy as f64 * y)
}

/// 2D simplex noise with the `BIOME_INFO_NOISE` table (offsets are zero).
/// Mirrors `SimplexNoise.get(double, double)` exactly, in `f64`, cast to
/// `f32` on return like the Java `(float)` cast.
pub fn simplex2(xin: f64, yin: f64) -> f32 {
    let f2 = 0.5 * (SQRT_3 - 1.0);
    let g2 = (3.0 - SQRT_3) / 6.0;
    let s = (xin + yin) * f2;
    let i = noise_floor(xin + s);
    let j = noise_floor(yin + s);
    let t = ((i + j) as f64) * g2;
    let x0 = xin - (i as f64 - t);
    let y0 = yin - (j as f64 - t);
    let (i1, j1) = if x0 > y0 { (1, 0) } else { (0, 1) };
    let x1 = x0 - i1 as f64 + g2;
    let y1 = y0 - j1 as f64 + g2;
    let x2 = x0 - 1.0 + 2.0 * g2;
    let y2 = y0 - 1.0 + 2.0 * g2;
    let ii = i & 0xFF;
    let jj = j & 0xFF;
    let gi0 = permute(ii + permute(jj) as i32) % 12;
    let gi1 = permute(ii + i1 + permute(jj + j1) as i32) % 12;
    let gi2 = permute(ii + 1 + permute(jj + 1) as i32) % 12;
    let n0 = corner_noise(gi0, x0, y0);
    let n1 = corner_noise(gi1, x1, y1);
    let n2 = corner_noise(gi2, x2, y2);
    (70.0 * (n0 + n1 + n2)) as f32
}

/// Correctly-rounded sqrt(3); the toolchain's `core` exposes no float
/// methods in `no_std`, so the single needed root is a verified constant
/// (pinned by the bit-exact JVM oracle test below).
const SQRT_3: f64 = 1.7320508075688772;

/// Exact floor for the small-magnitude inputs simplex uses.
/// (`as` truncates toward zero; negatives need the extra step.)
fn noise_floor(x: f64) -> i32 {
    let truncated = x as i32;
    if (truncated as f64) > x {
        truncated - 1
    } else {
        truncated
    }
}
pub const SWAMP_SCALE: f64 = 0.0225;
pub const SWAMP_THRESHOLD: f64 = -0.1;
pub const SWAMP_WET: [u8; 3] = mixed_rgb(-11766212);
pub const SWAMP_DRY: [u8; 3] = mixed_rgb(-9801671);

const fn mixed_rgb(argb: i32) -> [u8; 3] {
    let bits = argb as u32;
    [
        ((bits >> 16) & 0xFF) as u8,
        ((bits >> 8) & 0xFF) as u8,
        (bits & 0xFF) as u8,
    ]
}

/// Vanilla `GrassColorModifier.SWAMP.modifyColor`: flat wet/dry colors picked
/// by `BIOME_INFO_NOISE` at world (x, z). The colormap base color is ignored.
pub fn swamp_grass_tint(x: i32, z: i32) -> [u8; 3] {
    let ground = simplex2(x as f64 * SWAMP_SCALE, z as f64 * SWAMP_SCALE);
    if f64::from(ground) < SWAMP_THRESHOLD {
        SWAMP_WET
    } else {
        SWAMP_DRY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle values sampled from a live JVM
    /// (`new SimplexNoise(new WorldgenRandom(new LegacyRandomSource(2345L)), true)`
    /// at block coords scaled by 0.0225).
    const ORACLE: [(i32, i32, f32); 10] = [
        (0, 0, 0.0),
        (1, 0, 0.09803943),
        (0, 1, 0.0),
        (-1, -3, -0.09451092),
        (100, 200, -0.7511575),
        (-500, 333, -0.7176619),
        (1234, -5678, 0.17188638),
        (42, 42, 0.49476114),
        (-7, 13, -0.3922392),
        (9999, 9999, 0.40396762),
    ];

    #[test]
    fn biome_info_noise_oracle() {
        for (x, z, expected) in ORACLE {
            let got = simplex2(x as f64 * SWAMP_SCALE, z as f64 * SWAMP_SCALE);
            assert_eq!(
                got, expected,
                "noise mismatch at ({x}, {z}): got {got}, want {expected}"
            );
        }
    }

    #[test]
    fn swamp_tint_picks_both_sides_of_threshold() {
        assert_eq!(swamp_grass_tint(100, 200), SWAMP_WET);
        assert_eq!(swamp_grass_tint(42, 42), SWAMP_DRY);
        assert_eq!(SWAMP_WET, [76, 118, 60]);
        assert_eq!(SWAMP_DRY, [106, 112, 57]);
    }
}
