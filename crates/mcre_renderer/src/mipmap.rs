//! Port of vanilla's mipmap generation: `MipmapGenerator.generateMipLevels`
//! with `MipmapStrategy.AUTO` reduced to `MEAN`/`CUTOUT`, plus the
//! `TextureUtil.solidify` pre-pass applied to cutout sprites.

use std::collections::VecDeque;
use std::sync::LazyLock;

/// `Options.mipmapLevels`: levels generated above the base image.
pub const MIP_LEVELS: u32 = 4;

/// `MipmapGenerator.ALPHA_CUTOFF` (`STRICT_CUTOUT` would use 0.3).
pub const ALPHA_CUTOFF: f32 = 0.5;

/// `ARGB.SRGB_TO_LINEAR`: 10-bit linear channels indexed by an sRGB byte.
static SRGB_TO_LINEAR: LazyLock<[u16; 256]> = LazyLock::new(|| {
    std::array::from_fn(|index| {
        let channel = index as f32 / 255.0;
        (srgb_to_linear(channel) * 1023.0).round() as u16
    })
});

/// `ARGB.LINEAR_TO_SRGB`: sRGB bytes indexed by a 10-bit linear channel.
static LINEAR_TO_SRGB: LazyLock<[u8; 1024]> = LazyLock::new(|| {
    std::array::from_fn(|index| {
        let channel = index as f32 / 1023.0;
        (linear_to_srgb(channel) * 255.0).round() as u8
    })
});

fn srgb_to_linear(channel: f32) -> f32 {
    if channel >= 0.040_45 {
        ((channel + 0.055) / 1.055).powf(2.4)
    } else {
        channel / 12.92
    }
}

fn linear_to_srgb(channel: f32) -> f32 {
    if channel >= 0.003_130_8 {
        1.055 * channel.powf(1.0 / 2.4) - 0.055
    } else {
        12.92 * channel
    }
}

/// `NativeImage.computeTransparency().hasTransparent()`: any fully clear texel.
pub fn has_transparent(rgba: &[u8]) -> bool {
    rgba.as_chunks::<4>().0.iter().any(|pixel| pixel[3] == 0)
}

/// `TextureUtil.solidify`: bleed the nearest opaque color under transparent
/// texels so mip levels do not pull black into the cutout edges.
pub fn solidify(width: u32, height: u32, rgba: &mut [u8]) {
    let width = width as usize;
    let height = height as usize;
    let mut nearest = vec![[0_u8; 3]; width * height];
    let mut distance = vec![u32::MAX; width * height];
    let mut queue = VecDeque::new();
    for y in 0..height {
        for x in 0..width {
            let index = (y * width + x) * 4;
            if rgba[index + 3] != 0 {
                let cell = y * width + x;
                distance[cell] = 0;
                nearest[cell] = [rgba[index], rgba[index + 1], rgba[index + 2]];
                queue.push_back(cell);
            }
        }
    }
    while let Some(cell) = queue.pop_front() {
        let x = cell % width;
        let y = cell / width;
        for (dx, dy) in [(1_isize, 0_isize), (-1, 0), (0, 1), (0, -1)] {
            let nx = x as isize + dx;
            let ny = y as isize + dy;
            if nx < 0 || ny < 0 || nx >= width as isize || ny >= height as isize {
                continue;
            }
            let neighbor = ny as usize * width + nx as usize;
            if distance[neighbor] > distance[cell] + 1 {
                distance[neighbor] = distance[cell] + 1;
                nearest[neighbor] = nearest[cell];
                queue.push_back(neighbor);
            }
        }
    }
    for y in 0..height {
        for x in 0..width {
            let index = (y * width + x) * 4;
            if rgba[index + 3] == 0 {
                let color = nearest[y * width + x];
                rgba[index] = color[0];
                rgba[index + 1] = color[1];
                rgba[index + 2] = color[2];
            }
        }
    }
}

/// `MipmapGenerator.alphaTestCoverage`: fraction of 4x4 bilinear subsamples
/// per 2x2 texel cell whose alpha exceeds `alpha_ref`.
pub fn alpha_test_coverage(
    width: u32,
    height: u32,
    rgba: &[u8],
    alpha_ref: f32,
    alpha_scale: f32,
) -> f32 {
    let width = width as usize;
    let height = height as usize;
    if width < 2 || height < 2 {
        return f32::NAN;
    }
    let alpha = |x: usize, y: usize| {
        (f32::from(rgba[(y * width + x) * 4 + 3]) / 255.0 * alpha_scale).clamp(0.0, 1.0)
    };
    let mut coverage = 0.0_f32;
    for y in 0..height - 1 {
        for x in 0..width - 1 {
            let a00 = alpha(x, y);
            let a10 = alpha(x + 1, y);
            let a01 = alpha(x, y + 1);
            let a11 = alpha(x + 1, y + 1);
            let mut texel_coverage = 0.0_f32;
            for subsample_y in 0..4 {
                let fy = (subsample_y as f32 + 0.5) / 4.0;
                for subsample_x in 0..4 {
                    let fx = (subsample_x as f32 + 0.5) / 4.0;
                    let alpha = a00 * (1.0 - fx) * (1.0 - fy)
                        + a10 * fx * (1.0 - fy)
                        + a01 * (1.0 - fx) * fy
                        + a11 * fx * fy;
                    if alpha > alpha_ref {
                        texel_coverage += 1.0;
                    }
                }
            }
            coverage += texel_coverage / 16.0;
        }
    }
    coverage / ((width - 1) as f32 * (height - 1) as f32)
}

/// `MipmapGenerator.scaleAlphaToCoverage`: find the alpha scale that restores
/// the base level's cutout coverage, then bias the result.
fn scale_alpha_to_coverage(
    width: u32,
    height: u32,
    rgba: &mut [u8],
    desired_coverage: f32,
    alpha_cutoff_bias: f32,
) {
    let (mut min_scale, mut max_scale, mut alpha_scale) = (0.0_f32, 4.0_f32, 1.0_f32);
    let mut best_scale = 1.0_f32;
    let mut best_error = f32::MAX;
    for _ in 0..5 {
        let current = alpha_test_coverage(width, height, rgba, ALPHA_CUTOFF, alpha_scale);
        let error = (current - desired_coverage).abs();
        if error < best_error {
            best_error = error;
            best_scale = alpha_scale;
        }
        if current < desired_coverage {
            min_scale = alpha_scale;
        } else if current > desired_coverage {
            max_scale = alpha_scale;
        } else {
            break;
        }
        alpha_scale = (min_scale + max_scale) * 0.5;
    }
    for pixel in rgba.as_chunks_mut::<4>().0 {
        let alpha = f32::from(pixel[3]) / 255.0;
        let alpha = (alpha * best_scale + alpha_cutoff_bias + 0.025).clamp(0.0, 1.0);
        pixel[3] = (alpha * 255.0).floor() as u8;
    }
}

/// `ARGB.meanLinear`: integer alpha mean, linear-space channel mean.
fn mean_linear(colors: [[u8; 4]; 4]) -> [u8; 4] {
    let alpha = colors.iter().map(|color| u32::from(color[3])).sum::<u32>() / 4;
    let mut out = [0_u8; 4];
    out[3] = alpha as u8;
    for channel in 0..3 {
        let linear = colors
            .iter()
            .map(|color| u32::from(SRGB_TO_LINEAR[color[channel] as usize]))
            .sum::<u32>()
            / 4;
        out[channel] = LINEAR_TO_SRGB[linear as usize];
    }
    out
}

/// One level of the chain: 2x2 `meanLinear`, then coverage correction for
/// cutout sprites. `original_coverage` is the base level's coverage.
pub fn next_level(
    width: u32,
    height: u32,
    rgba: &[u8],
    cutout: bool,
    original_coverage: f32,
    alpha_cutoff_bias: f32,
) -> (u32, u32, Vec<u8>) {
    let next_width = (width >> 1).max(1);
    let next_height = (height >> 1).max(1);
    let mut next = vec![0_u8; (next_width * next_height * 4) as usize];
    let width = width as usize;
    let height = height as usize;
    for y in 0..next_height as usize {
        for x in 0..next_width as usize {
            let sample = |sx: usize, sy: usize| {
                let px = (x * 2 + sx).min(width - 1);
                let py = (y * 2 + sy).min(height - 1);
                let index = (py * width + px) * 4;
                [
                    rgba[index],
                    rgba[index + 1],
                    rgba[index + 2],
                    rgba[index + 3],
                ]
            };
            let color = mean_linear([sample(0, 0), sample(1, 0), sample(0, 1), sample(1, 1)]);
            let index = (y * next_width as usize + x) * 4;
            next[index..index + 4].copy_from_slice(&color);
        }
    }
    if cutout {
        scale_alpha_to_coverage(
            next_width,
            next_height,
            &mut next,
            original_coverage,
            alpha_cutoff_bias,
        );
    }
    (next_width, next_height, next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_tables_match_vanilla_endpoints() {
        assert_eq!(SRGB_TO_LINEAR[0], 0);
        assert_eq!(SRGB_TO_LINEAR[255], 1023);
        assert_eq!(LINEAR_TO_SRGB[0], 0);
        assert_eq!(LINEAR_TO_SRGB[1023], 255);
    }

    #[test]
    fn mean_linear_averages_in_linear_space() {
        let black = [0, 0, 0, 255];
        let white = [255, 255, 255, 255];
        let mean = mean_linear([black, white, black, white]);
        assert_eq!(mean[3], 255);
        assert_eq!(mean[0], LINEAR_TO_SRGB[SRGB_TO_LINEAR[255] as usize / 2]);
        assert!(mean[0] > 127);
    }

    #[test]
    fn solidify_bleeds_opaque_color_under_clear_texels() {
        let mut rgba = vec![0_u8; 4 * 4 * 4];
        rgba[0..4].copy_from_slice(&[10, 20, 30, 255]);
        solidify(4, 4, &mut rgba);
        assert_eq!(&rgba[4..8], &[10, 20, 30, 0]);
        assert_eq!(&rgba[60..64], &[10, 20, 30, 0]);
    }

    #[test]
    fn cutout_levels_keep_base_coverage() {
        let mut rgba = vec![0_u8; 16 * 16 * 4];
        for y in 0..16 {
            for x in 0..16 {
                let index = (y * 16 + x) * 4;
                let opaque = x < 8;
                rgba[index..index + 4].copy_from_slice(&[255, 255, 255, u8::from(opaque) * 255]);
            }
        }
        let base = alpha_test_coverage(16, 16, &rgba, ALPHA_CUTOFF, 1.0);
        let (width, height, level) = next_level(16, 16, &rgba, true, base, 0.0);
        assert_eq!((width, height), (8, 8));
        let coverage = alpha_test_coverage(width, height, &level, ALPHA_CUTOFF, 1.0);
        assert!((coverage - base).abs() < 0.05);
    }

    #[test]
    fn tiny_levels_do_not_panic() {
        let rgba = vec![255_u8; 4];
        let (width, height, level) = next_level(1, 1, &rgba, true, 0.0, 0.0);
        assert_eq!((width, height), (1, 1));
        assert_eq!(level.len(), 4);
    }
}
