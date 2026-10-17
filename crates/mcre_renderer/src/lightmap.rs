//! CPU port of vanilla's `lightmap.fsh` (26.4).
//!
//! The lightmap is a 16x16 texture indexed by raw `(block, sky)` light
//! levels. Vanilla bakes the same values into a 256x256 texture (each level
//! pair fills a 16x16 region of constant color) and samples it per vertex in
//! `block.vsh`/`terrain.vsh` via `sample_lightmap`; a 16x16 texture sampled
//! with `textureLoad` at the level indices is exactly equivalent.
//!
//! Deliberately not ported (again matching the vanilla defaults this client
//! cannot reach): night vision, boss overlay darkening, darkness effect,
//! end-flash sky boost, and the block-light flicker term.

/// `EnvironmentAttributes.BLOCK_LIGHT_TINT` default, `RGB24(-10100)`.
pub const BLOCK_LIGHT_TINT: [f32; 3] = [1.0, 216.0 / 255.0, 140.0 / 255.0];
/// `EnvironmentAttributes.SKY_LIGHT_COLOR` default, `RGB24(-1)` (white).
pub const SKY_LIGHT_COLOR: [f32; 3] = [1.0, 1.0, 1.0];
/// `EnvironmentAttributes.AMBIENT_LIGHT_COLOR` default, `RGB24(-16777216)`.
pub const AMBIENT_COLOR: [f32; 3] = [0.0, 0.0, 0.0];
/// `LightmapRenderStateExtractor.blockFactor` floor: `flicker + 1.4`.
pub const BLOCK_LIGHT_FACTOR: f32 = 1.4;
/// Vanilla `Options.gamma` default.
pub const DEFAULT_BRIGHTNESS: f32 = 0.5;

/// `lightmap.fsh get_brightness`: `v / (4 - 3v)` with `v = level / 15`.
pub fn brightness_curve(level: u8) -> f32 {
    let v = f32::from(level.min(15)) / 15.0;
    v / (4.0 - 3.0 * v)
}

fn not_gamma(color: [f32; 3]) -> [f32; 3] {
    let max = color[0].max(color[1]).max(color[2]);
    if max == 0.0 {
        return [0.0; 3];
    }
    let max_inverted = 1.0 - max;
    let max_scaled = 1.0 - max_inverted * max_inverted * max_inverted * max_inverted;
    let factor = max_scaled / max;
    [color[0] * factor, color[1] * factor, color[2] * factor]
}

/// `lightmap.fsh main` for one `(block, sky)` level pair. `sky_factor` is
/// the time-of-day multiplier (1.0 = noon), `block_factor` is
/// [`BLOCK_LIGHT_FACTOR`] plus flicker, `brightness` is the gamma option.
pub fn lightmap_color(
    block_level: u8,
    sky_level: u8,
    sky_factor: f32,
    block_factor: f32,
    brightness: f32,
) -> [f32; 3] {
    let block_v = f32::from(block_level.min(15)) / 15.0;

    let block_brightness = brightness_curve(block_level) * block_factor;
    let sky_brightness = brightness_curve(sky_level) * sky_factor;

    let parabolic = (2.0 * block_v - 1.0) * (2.0 * block_v - 1.0);
    let mix = 0.9 * parabolic;
    let block_light_color = [
        BLOCK_LIGHT_TINT[0] + (1.0 - BLOCK_LIGHT_TINT[0]) * mix,
        BLOCK_LIGHT_TINT[1] + (1.0 - BLOCK_LIGHT_TINT[1]) * mix,
        BLOCK_LIGHT_TINT[2] + (1.0 - BLOCK_LIGHT_TINT[2]) * mix,
    ];

    let mut color = [0.0; 3];
    for i in 0..3 {
        color[i] = AMBIENT_COLOR[i]
            + SKY_LIGHT_COLOR[i] * sky_brightness
            + block_light_color[i] * block_brightness;
        color[i] = color[i].clamp(0.0, 1.0);
    }

    let gamma = not_gamma(color);
    [
        color[0] + (gamma[0] - color[0]) * brightness,
        color[1] + (gamma[1] - color[1]) * brightness,
        color[2] + (gamma[2] - color[2]) * brightness,
    ]
}

fn to_unorm8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Bakes the full 16x16 RGBA8 lightmap: `x` is the block level, `y` the sky
/// level, matching the `(u, v)` axes of vanilla's lightmap texture.
pub fn lightmap_rgba(sky_factor: f32, block_factor: f32, brightness: f32) -> [[u8; 4]; 256] {
    std::array::from_fn(|i| {
        let color = lightmap_color(
            (i % 16) as u8,
            (i / 16) as u8,
            sky_factor,
            block_factor,
            brightness,
        );
        [
            to_unorm8(color[0]),
            to_unorm8(color[1]),
            to_unorm8(color[2]),
            255,
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_curve_matches_java_lightmap() {
        assert_eq!(brightness_curve(0), 0.0);
        assert_eq!(brightness_curve(15), 1.0);
        let cases = [
            (1, 0.01754),
            (5, 0.11111),
            (7, 0.17949),
            (8, 0.22222),
            (10, 0.33333),
            (14, 0.77778),
        ];
        for (level, expected) in cases {
            assert!(
                (brightness_curve(level) - expected).abs() < 1e-4,
                "level {level}: got {}, want {expected}",
                brightness_curve(level),
            );
        }
    }

    #[test]
    fn full_daylight_sky_level_is_white() {
        let color = lightmap_color(0, 15, 1.0, BLOCK_LIGHT_FACTOR, DEFAULT_BRIGHTNESS);
        assert_eq!(color, [1.0, 1.0, 1.0]);
    }

    #[test]
    fn zero_levels_are_black() {
        for brightness in [0.0, 0.5, 1.0] {
            let color = lightmap_color(0, 0, 1.0, BLOCK_LIGHT_FACTOR, brightness);
            assert_eq!(color, [0.0, 0.0, 0.0], "brightness {brightness}");
        }
    }

    #[test]
    fn full_block_level_clamps_to_white() {
        // get_brightness(15/15) * 1.4 > 1: vanilla overbright clamps to
        // white, so the warm tint is invisible at maximum block light.
        let color = lightmap_color(15, 0, 1.0, BLOCK_LIGHT_FACTOR, 0.0);
        assert_eq!(color, [1.0, 1.0, 1.0]);
    }

    #[test]
    fn mid_block_level_shows_warm_tint() {
        // Level 12: curve 0.5 * 1.4 = 0.7, parabolic mix 0.9 * 0.36 = 0.324.
        let color = lightmap_color(12, 0, 1.0, BLOCK_LIGHT_FACTOR, 0.0);
        let expect = [0.7, 0.627_628, 0.486_596];
        for i in 0..3 {
            assert!(
                (color[i] - expect[i]).abs() < 1e-5,
                "channel {i}: got {}, want {}",
                color[i],
                expect[i],
            );
        }
    }

    #[test]
    fn brightness_lifts_midtones_toward_not_gamma() {
        let dim = lightmap_color(8, 0, 1.0, BLOCK_LIGHT_FACTOR, 0.0);
        let gamma = lightmap_color(8, 0, 1.0, BLOCK_LIGHT_FACTOR, 1.0);
        let half = lightmap_color(8, 0, 1.0, BLOCK_LIGHT_FACTOR, 0.5);
        assert!(gamma[0] > dim[0]);
        assert!((half[0] - (dim[0] + gamma[0]) * 0.5).abs() < 1e-6);
    }

    #[test]
    fn texture_layout_maps_x_to_block_y_to_sky() {
        let pixels = lightmap_rgba(1.0, BLOCK_LIGHT_FACTOR, DEFAULT_BRIGHTNESS);
        assert_eq!(pixels[0], [0, 0, 0, 255]);
        assert_eq!(pixels[15], [255, 255, 255, 255]);
        assert_eq!(&pixels[15 * 16..16 * 16], &[[255, 255, 255, 255]; 16][..]);
        for (index, pixel) in pixels.iter().enumerate() {
            let block = (index % 16) as u8;
            let sky = (index / 16) as u8;
            let color = lightmap_color(block, sky, 1.0, BLOCK_LIGHT_FACTOR, DEFAULT_BRIGHTNESS);
            assert_eq!(
                *pixel,
                [
                    to_unorm8(color[0]),
                    to_unorm8(color[1]),
                    to_unorm8(color[2]),
                    255,
                ],
            );
        }
    }
}
