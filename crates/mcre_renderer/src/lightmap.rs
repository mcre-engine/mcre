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
/// Overworld `DimensionTypes` ambient light color (`RGB24(-16119286)`).
pub const AMBIENT_COLOR: [f32; 3] = [10.0 / 255.0; 3];
/// `LightmapRenderStateExtractor.blockFactor` floor: `flicker + 1.4`.
pub const BLOCK_LIGHT_FACTOR: f32 = 1.4;
/// Vanilla `Options.gamma` default.
pub const DEFAULT_BRIGHTNESS: f32 = 0.5;

/// Ticks in one overworld day (`Timeline` period).
pub const DAY_LENGTH_TICKS: f32 = 24000.0;
/// `Timelines.NIGHT_SKY_LIGHT_FACTOR`.
pub const NIGHT_SKY_LIGHT_FACTOR: f32 = 0.24;
/// `Timelines.NIGHT_SKY_LIGHT_COLOR`, `ARGB.colorFromFloat(1.0, 0.48, 0.48,
/// 1.0)` quantized by the timeline's `ARGB.vector3fFromRGB24` round-trip.
pub const NIGHT_SKY_LIGHT_COLOR: [f32; 3] = [122.0 / 255.0, 122.0 / 255.0, 1.0];

/// `Timelines.OVERWORLD_DAY` `SKY_LIGHT_FACTOR` track keyframes
/// `(tick, multiplier)`.
const SKY_LIGHT_FACTOR_KEYFRAMES: [(f32, f32); 4] = [
    (730.0, 1.0),
    (11270.0, 1.0),
    (13140.0, NIGHT_SKY_LIGHT_FACTOR),
    (22860.0, NIGHT_SKY_LIGHT_FACTOR),
];

/// `Timelines.OVERWORLD_DAY` `SKY_LIGHT_COLOR` track keyframes
/// `(tick, multiplier)`.
const SKY_LIGHT_COLOR_KEYFRAMES: [(f32, [f32; 3]); 4] = [
    (730.0, [1.0, 1.0, 1.0]),
    (11270.0, [1.0, 1.0, 1.0]),
    (13140.0, NIGHT_SKY_LIGHT_COLOR),
    (22860.0, NIGHT_SKY_LIGHT_COLOR),
];

/// Linear keyframe track with the day period wrapping from the last
/// keyframe back to the first.
fn track(time_ticks: f32, keyframes: &[(f32, f32)]) -> f32 {
    let time = time_ticks.rem_euclid(DAY_LENGTH_TICKS);
    let count = keyframes.len();
    for index in 0..count {
        let (start, start_value) = keyframes[index];
        let (mut end, end_value) = keyframes[(index + 1) % count];
        if index + 1 == count {
            end += DAY_LENGTH_TICKS;
        }
        let sample = if index + 1 == count && time < keyframes[0].0 {
            time + DAY_LENGTH_TICKS
        } else {
            time
        };
        if sample >= start && sample < end {
            let progress = (sample - start) / (end - start);
            return start_value + (end_value - start_value) * progress;
        }
    }
    keyframes[0].1
}

/// Vanilla overworld `SKY_LIGHT_FACTOR` at `time_ticks` (0 = `WAKE_UP_FROM_SLEEP`).
pub fn sky_light_factor(time_ticks: f32) -> f32 {
    track(time_ticks, &SKY_LIGHT_FACTOR_KEYFRAMES)
}

/// Vanilla overworld `SKY_LIGHT_COLOR` multiplier at `time_ticks`.
pub fn sky_light_color(time_ticks: f32) -> [f32; 3] {
    let mut color = [0.0; 3];
    for (channel, slot) in color.iter_mut().enumerate() {
        *slot = track(
            time_ticks,
            &SKY_LIGHT_COLOR_KEYFRAMES.map(|(tick, value)| (tick, value[channel])),
        );
    }
    color
}

/// Inputs to one lightmap bake: the time-of-day sky multiplier and color
/// plus the user-facing block factor and gamma option.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightmapParams {
    /// `LightmapInfo.SkyFactor`: [`sky_light_factor`].
    pub sky_factor: f32,
    /// `LightmapInfo.SkyLightColor`: [`sky_light_color`].
    pub sky_light_color: [f32; 3],
    /// `LightmapInfo.BlockFactor`: [`BLOCK_LIGHT_FACTOR`] plus flicker.
    pub block_factor: f32,
    /// Vanilla `Options.gamma`.
    pub brightness: f32,
}

impl LightmapParams {
    /// Noon with vanilla defaults.
    pub fn noon() -> Self {
        let time = 6000.0;
        Self {
            sky_factor: sky_light_factor(time),
            sky_light_color: sky_light_color(time),
            block_factor: BLOCK_LIGHT_FACTOR,
            brightness: DEFAULT_BRIGHTNESS,
        }
    }
}

impl Default for LightmapParams {
    fn default() -> Self {
        Self::noon()
    }
}

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

/// `lightmap.fsh main` for one `(block, sky)` level pair.
pub fn lightmap_color(block_level: u8, sky_level: u8, params: &LightmapParams) -> [f32; 3] {
    let block_v = f32::from(block_level.min(15)) / 15.0;

    let block_brightness = brightness_curve(block_level) * params.block_factor;
    let sky_brightness = brightness_curve(sky_level) * params.sky_factor;

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
            + params.sky_light_color[i] * sky_brightness
            + block_light_color[i] * block_brightness;
        color[i] = color[i].clamp(0.0, 1.0);
    }

    let gamma = not_gamma(color);
    [
        color[0] + (gamma[0] - color[0]) * params.brightness,
        color[1] + (gamma[1] - color[1]) * params.brightness,
        color[2] + (gamma[2] - color[2]) * params.brightness,
    ]
}

fn to_unorm8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Bakes the full 16x16 RGBA8 lightmap: `x` is the block level, `y` the sky
/// level, matching the `(u, v)` axes of vanilla's lightmap texture.
pub fn lightmap_rgba(params: &LightmapParams) -> [[u8; 4]; 256] {
    std::array::from_fn(|i| {
        let color = lightmap_color((i % 16) as u8, (i / 16) as u8, params);
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
        let color = lightmap_color(0, 15, &LightmapParams::default());
        assert_eq!(color, [1.0, 1.0, 1.0]);
    }

    #[test]
    fn zero_levels_use_overworld_ambient_light() {
        let params = LightmapParams {
            brightness: 0.0,
            ..LightmapParams::default()
        };
        assert_eq!(lightmap_color(0, 0, &params), AMBIENT_COLOR);
    }

    #[test]
    fn full_block_level_clamps_to_white() {
        // get_brightness(15/15) * 1.4 > 1: vanilla overbright clamps to
        // white, so the warm tint is invisible at maximum block light.
        let params = LightmapParams {
            brightness: 0.0,
            ..LightmapParams::default()
        };
        let color = lightmap_color(15, 0, &params);
        assert_eq!(color, [1.0, 1.0, 1.0]);
    }

    #[test]
    fn mid_block_level_shows_warm_tint() {
        // Level 12: curve 0.5 * 1.4 = 0.7, parabolic mix 0.9 * 0.36 = 0.324.
        let params = LightmapParams {
            brightness: 0.0,
            ..LightmapParams::default()
        };
        let color = lightmap_color(12, 0, &params);
        let expect = [0.739_215_7, 0.666_843_9, 0.525_811_8];
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
        let dim = lightmap_color(
            8,
            0,
            &LightmapParams {
                brightness: 0.0,
                ..LightmapParams::default()
            },
        );
        let gamma = lightmap_color(
            8,
            0,
            &LightmapParams {
                brightness: 1.0,
                ..LightmapParams::default()
            },
        );
        let half = lightmap_color(
            8,
            0,
            &LightmapParams {
                brightness: 0.5,
                ..LightmapParams::default()
            },
        );
        assert!(gamma[0] > dim[0]);
        assert!((half[0] - (dim[0] + gamma[0]) * 0.5).abs() < 1e-6);
    }

    #[test]
    fn texture_layout_maps_x_to_block_y_to_sky() {
        let params = LightmapParams::default();
        let pixels = lightmap_rgba(&params);
        assert_eq!(pixels[0], [24, 24, 24, 255]);
        assert_eq!(pixels[15], [255, 255, 255, 255]);
        assert_eq!(&pixels[15 * 16..16 * 16], &[[255, 255, 255, 255]; 16][..]);
        for (index, pixel) in pixels.iter().enumerate() {
            let block = (index % 16) as u8;
            let sky = (index / 16) as u8;
            let color = lightmap_color(block, sky, &params);
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

    #[test]
    fn day_night_timeline_matches_vanilla_keyframes() {
        assert_eq!(sky_light_factor(6000.0), 1.0);
        assert_eq!(sky_light_factor(11270.0), 1.0);
        assert_eq!(sky_light_factor(13140.0), NIGHT_SKY_LIGHT_FACTOR);
        assert_eq!(sky_light_factor(18000.0), NIGHT_SKY_LIGHT_FACTOR);
        assert_eq!(sky_light_factor(22860.0), NIGHT_SKY_LIGHT_FACTOR);

        let night_color = sky_light_color(18000.0);
        assert_eq!(night_color, NIGHT_SKY_LIGHT_COLOR);
        assert_eq!(sky_light_color(6000.0), [1.0, 1.0, 1.0]);

        let mid = sky_light_factor(12205.0);
        assert!((mid - (1.0 + NIGHT_SKY_LIGHT_FACTOR) * 0.5).abs() < 1e-5);
        let dawn = sky_light_factor(23795.0);
        assert!((dawn - (1.0 + NIGHT_SKY_LIGHT_FACTOR) * 0.5).abs() < 1e-5);
    }

    #[test]
    fn day_night_timeline_wraps_at_day_length() {
        for time in [0.0, 100.0, 500.0, 729.0] {
            assert_eq!(
                sky_light_factor(time),
                sky_light_factor(time + DAY_LENGTH_TICKS)
            );
            assert_eq!(
                sky_light_color(time),
                sky_light_color(time + DAY_LENGTH_TICKS)
            );
        }
        assert_eq!(sky_light_factor(0.0), sky_light_factor(DAY_LENGTH_TICKS));
        assert!(sky_light_factor(0.0) > NIGHT_SKY_LIGHT_FACTOR);
        assert!(sky_light_factor(0.0) < 1.0);
    }

    #[test]
    fn night_sky_light_is_dim_blue_moonlight() {
        let night = LightmapParams {
            sky_factor: sky_light_factor(18000.0),
            sky_light_color: sky_light_color(18000.0),
            ..LightmapParams::default()
        };
        let color = lightmap_color(0, 15, &night);
        assert!(color[2] > color[0], "moonlight is blue-tinted: {color:?}");
        assert!(color[0] > 0.05, "moonlit ground stays visible: {color:?}");
        assert!(color[0] < 0.35, "moonlight stays dim: {color:?}");
    }
}
