use crate::Fog;
use crate::lightmap::DAY_LENGTH_TICKS;
use mcre_world::fluid::FluidKind;

/// Vanilla `EnvironmentAttributes.FOG_COLOR` for the overworld (`#c0d8ff`).
pub const OVERWORLD_FOG_COLOR: [f32; 3] = [192.0 / 255.0, 216.0 / 255.0, 1.0];
/// Vanilla `EnvironmentAttributes.SKY_COLOR` for the overworld (`#78a7ff`).
pub const OVERWORLD_SKY_COLOR: [f32; 3] = [120.0 / 255.0, 167.0 / 255.0, 1.0];
/// Vanilla `EnvironmentAttributes.SKY_FOG_END_DISTANCE`.
pub const SKY_FOG_END_DISTANCE: f32 = 512.0;
/// Vanilla `Timelines.NIGHT_CLOUD_COLOR_MULTIPLIER`.
pub const NIGHT_CLOUD_COLOR: [f32; 3] = [0.1, 0.1, 0.15];
/// Vanilla `EnvironmentAttributes.WATER_FOG_COLOR` default (`#050533`).
pub const WATER_FOG_COLOR: [f32; 3] = [5.0 / 255.0, 5.0 / 255.0, 51.0 / 255.0];
/// Vanilla `LavaFogEnvironment.COLOR` (`#991900`).
pub const LAVA_FOG_COLOR: [f32; 3] = [153.0 / 255.0, 25.0 / 255.0, 0.0];

/// Vanilla `Timelines.OVERWORLD_DAY` `visual/fog_color` multiplier track
/// (`multiply`), white by day and `#0f0f16` at night.
pub const FOG_COLOR_MULTIPLIER: [(f32, [f32; 3]); 4] = [
    (133.0, [1.0, 1.0, 1.0]),
    (11867.0, [1.0, 1.0, 1.0]),
    (13670.0, [15.0 / 255.0, 15.0 / 255.0, 22.0 / 255.0]),
    (22330.0, [15.0 / 255.0, 15.0 / 255.0, 22.0 / 255.0]),
];

/// Vanilla `Timelines.OVERWORLD_DAY` `visual/sky_color` multiplier track.
pub const SKY_COLOR_MULTIPLIER: [(f32, [f32; 3]); 4] = [
    (133.0, [1.0, 1.0, 1.0]),
    (11867.0, [1.0, 1.0, 1.0]),
    (13670.0, [0.0, 0.0, 0.0]),
    (22330.0, [0.0, 0.0, 0.0]),
];

/// Linear keyframe track with the day wrapping from the last keyframe back
/// to the first, mirroring `Timeline`'s default interpolation.
pub fn time_multiplier(time_ticks: f32, keys: &[(f32, [f32; 3])]) -> [f32; 3] {
    let time = time_ticks.rem_euclid(DAY_LENGTH_TICKS);
    let count = keys.len();
    for index in 0..count {
        let (start, from) = keys[index];
        let (mut end, to) = keys[(index + 1) % count];
        if index + 1 == count {
            end += DAY_LENGTH_TICKS;
        }
        let sample = if index + 1 == count && time < keys[0].0 {
            time + DAY_LENGTH_TICKS
        } else {
            time
        };
        if sample >= start && sample < end {
            let t = (sample - start) / (end - start);
            return [
                from[0] + (to[0] - from[0]) * t,
                from[1] + (to[1] - from[1]) * t,
                from[2] + (to[2] - from[2]) * t,
            ];
        }
    }
    keys[0].1
}

/// Vanilla `AtmosphericFogEnvironment.getBaseColor`: the dimension's fog
/// color pulled toward its sky color as the sky fog end nears.
pub fn atmospheric_fog_color(render_distance: u32, time_ticks: f32) -> [f32; 3] {
    let sky_end = (SKY_FOG_END_DISTANCE / 16.0).min(render_distance as f32);
    let lerp = (sky_end / 32.0).clamp(0.0, 1.0) * (1.0 - 0.25) + 0.25;
    let mix = 1.0 - lerp.powf(0.25);
    let fog = time_multiplier(time_ticks, &FOG_COLOR_MULTIPLIER);
    let sky = time_multiplier(time_ticks, &SKY_COLOR_MULTIPLIER);
    let mut color = [0.0; 3];
    for i in 0..3 {
        let fog = OVERWORLD_FOG_COLOR[i] * fog[i];
        let sky = OVERWORLD_SKY_COLOR[i] * sky[i];
        color[i] = fog + (sky - fog) * mix;
    }
    color
}

/// Vanilla `FogRenderer.setupFog`: the environmental fog follows the
/// dimension attributes (the overworld's default span is far past the loaded
/// radius), while the render-distance fog hides the loaded edge.
pub fn vanilla_fog(
    render_distance: u32,
    time_ticks: f32,
    submerged: Option<FluidKind>,
    water_vision: f32,
) -> Fog {
    let render_distance_blocks = render_distance as f32 * 16.0;
    let span = (render_distance_blocks / 10.0).clamp(4.0, 64.0);
    let (color, environmental_start, environmental_end) = match submerged {
        Some(FluidKind::Water) => {
            let vision = water_vision.clamp(0.0, 1.0);
            (
                brightened_water_fog_color(vision),
                -8.0,
                96.0 * vision.max(0.25),
            )
        }
        Some(FluidKind::Lava) => (LAVA_FOG_COLOR, 0.25, 1.0),
        None => (
            atmospheric_fog_color(render_distance, time_ticks),
            0.0,
            1024.0,
        ),
    };
    Fog {
        color,
        density: 1.0,
        environmental_start,
        environmental_end,
        render_distance_start: render_distance_blocks - span,
        render_distance_end: render_distance_blocks,
    }
}

/// Vanilla `FogRenderer.computeFogColor` water branch: lerp every channel
/// toward `channel * (1 / maxChannel)` by the player's water vision.
fn brightened_water_fog_color(water_vision: f32) -> [f32; 3] {
    let mut color = WATER_FOG_COLOR;
    let scale = 1.0 / color[2].max(color[1]).max(color[0]);
    for channel in &mut color {
        *channel += (*channel * scale - *channel) * water_vision;
    }
    color
}

/// Approximation of the vanilla `CLOUD_COLOR` timeline keyed on the same
/// day/night scalar as the fog: white at day, dark blue-grey at night, with
/// the overworld's 0.8 alpha.
pub fn cloud_color_for(sky: f32) -> [f32; 4] {
    let t = sky.clamp(0.0, 1.0);
    [
        NIGHT_CLOUD_COLOR[0] + (1.0 - NIGHT_CLOUD_COLOR[0]) * t,
        NIGHT_CLOUD_COLOR[1] + (1.0 - NIGHT_CLOUD_COLOR[1]) * t,
        NIGHT_CLOUD_COLOR[2] + (1.0 - NIGHT_CLOUD_COLOR[2]) * t,
        0.8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn water_fog_uses_minimum_vision_and_vanilla_brightening() {
        let fog = vanilla_fog(8, 6000.0, Some(FluidKind::Water), 0.0);
        assert_eq!(fog.environmental_start, -8.0);
        assert_eq!(fog.environmental_end, 24.0);
        assert_eq!(fog.color, WATER_FOG_COLOR);

        let fog = vanilla_fog(8, 6000.0, Some(FluidKind::Water), 0.5);
        assert_eq!(fog.environmental_end, 48.0);
        assert!((fog.color[2] - 0.6).abs() < 1.0e-6);
        assert!((fog.color[0] - 0.058_823_53).abs() < 1.0e-6);

        // Exact vanilla sample from the river probe at water vision 0.33:
        // #050533 lerped toward #0c0cff by that amount.
        let fog = vanilla_fog(8, 6000.0, Some(FluidKind::Water), 0.33);
        assert!((fog.color[0] - 0.045_490_2).abs() < 1.0e-6);
        assert!((fog.color[1] - 0.045_490_2).abs() < 1.0e-6);
        assert!((fog.color[2] - 0.464).abs() < 1.0e-6);
        assert!((fog.environmental_end - 31.68).abs() < 1.0e-5);
    }
}
