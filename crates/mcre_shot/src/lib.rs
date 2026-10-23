//! The shared parity-shot spec: one JSON file describing a camera and its
//! environment, consumed by both screenshot pipelines so the two resulting
//! PNGs line up pixel-for-pixel.
//!
//! Conventions:
//!
//! - `eye` is the camera eye position in absolute world coordinates. The
//!   vanilla pipeline stores it as the player's feet position using
//!   [`EYE_HEIGHT`], the mcre pipeline places the fly camera directly.
//! - `yaw`/`pitch` use the **vanilla** angle convention (degrees): yaw `0`
//!   faces south (+Z), positive yaw turns west (-X), pitch positive looks
//!   down. The mcre camera uses yaw `0` = north with positive turning east
//!   and positive pitch looking up, so the conversions in [`ShotSpec`] apply
//!   a 180-degree yaw offset and negate pitch.
//! - `fov` is the vertical field of view in degrees (70 = vanilla default),
//!   rendered exactly by both pipelines; `brightness` is the vanilla `gamma`
//!   option in `0.0..=1.0` (0.5 = default).
//! - `time_ticks` is the overworld day clock in `0..=24000` (6000 = noon),
//!   frozen in the vanilla world via a paused `world_clocks` entry.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Standing player eye height in 26.4 (`Avatar.DEFAULT_EYE_HEIGHT`), used to
/// turn the spec's eye position into the feet position vanilla loads.
pub const EYE_HEIGHT: f64 = 1.62;

const DEFAULT_FOV: f64 = 70.0;
const DEFAULT_BRIGHTNESS: f64 = 0.5;
const DEFAULT_TIME_TICKS: i64 = 6000;
const DEFAULT_WIDTH: u32 = 1280;
const DEFAULT_HEIGHT: u32 = 720;
const DEFAULT_RENDER_DISTANCE: u32 = 8;
const DEFAULT_SETTLE_MS: u64 = 4000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Clouds {
    #[default]
    Off,
    Fast,
    Fancy,
}

impl Clouds {
    pub fn as_str(self) -> &'static str {
        match self {
            Clouds::Off => "off",
            Clouds::Fast => "fast",
            Clouds::Fancy => "fancy",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShotSpec {
    /// Shot name; also the default output file stem.
    pub name: String,
    /// World save directory (region/level.dat), e.g. `target/gen_world/world`.
    pub world: PathBuf,
    /// Camera eye position (absolute world coordinates).
    pub eye: [f64; 3],
    /// Look direction, vanilla convention: degrees, 0 = south (+Z),
    /// positive turns west, pitch positive looks down.
    pub yaw: f64,
    /// Look direction pitch, vanilla convention (see [`Self::yaw`]).
    pub pitch: f64,
    /// Vanilla FOV option, degrees (default 70).
    #[serde(default = "default_fov")]
    pub fov: f64,
    /// Vanilla `gamma` option, `0.0..=1.0` (default 0.5).
    #[serde(default = "default_brightness")]
    pub brightness: f64,
    /// Overworld day clock, `0..=24000`, 6000 = noon (default 6000).
    #[serde(default = "default_time_ticks")]
    pub time_ticks: i64,
    /// Output resolution in physical pixels.
    #[serde(default = "default_width")]
    pub width: u32,
    /// Output resolution in physical pixels.
    #[serde(default = "default_height")]
    pub height: u32,
    /// Render distance in chunks; becomes the vanilla `renderDistance`
    /// option and the mcre stream ring radius (default 8).
    #[serde(default = "default_render_distance")]
    pub render_distance: u32,
    /// Cloud rendering. Keep this `off` for 1:1 shots: vanilla cloud drift
    /// is time-dependent and cannot be reproduced exactly (default off).
    #[serde(default)]
    pub clouds: Clouds,
    /// Vanilla only: settle time after the world is visible, in
    /// milliseconds, before the HUD is hidden and the shot is taken.
    #[serde(default = "default_settle_ms")]
    pub settle_ms: u64,
    /// Vanilla `ao` option: smooth lighting plus ambient occlusion. Disabling
    /// it switches both pipelines to flat lighting (default true).
    #[serde(default = "default_ao")]
    pub ao: bool,
}

fn default_fov() -> f64 {
    DEFAULT_FOV
}
fn default_brightness() -> f64 {
    DEFAULT_BRIGHTNESS
}
fn default_time_ticks() -> i64 {
    DEFAULT_TIME_TICKS
}
fn default_width() -> u32 {
    DEFAULT_WIDTH
}
fn default_height() -> u32 {
    DEFAULT_HEIGHT
}
fn default_render_distance() -> u32 {
    DEFAULT_RENDER_DISTANCE
}
fn default_settle_ms() -> u64 {
    DEFAULT_SETTLE_MS
}
fn default_ao() -> bool {
    true
}

#[derive(Debug, thiserror::Error)]
pub enum ShotError {
    #[error("failed to read spec {path}: {err}")]
    Read { path: PathBuf, err: std::io::Error },
    #[error("invalid spec {path}: {err}")]
    Parse {
        path: PathBuf,
        err: serde_json::Error,
    },
    #[error("invalid spec: {0}")]
    Invalid(String),
}

impl ShotSpec {
    pub fn load(path: &Path) -> Result<Self, ShotError> {
        let text = std::fs::read_to_string(path).map_err(|err| ShotError::Read {
            path: path.to_path_buf(),
            err,
        })?;
        let spec: Self = serde_json::from_str(&text).map_err(|err| ShotError::Parse {
            path: path.to_path_buf(),
            err,
        })?;
        spec.validate()?;
        Ok(spec)
    }

    pub fn save(&self, path: &Path) -> Result<(), ShotError> {
        let json = serde_json::to_string_pretty(self).map_err(|err| ShotError::Parse {
            path: path.to_path_buf(),
            err,
        })?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| ShotError::Read {
                path: path.to_path_buf(),
                err,
            })?;
        }
        std::fs::write(path, format!("{json}\n")).map_err(|err| ShotError::Read {
            path: path.to_path_buf(),
            err,
        })
    }

    pub fn validate(&self) -> Result<(), ShotError> {
        let invalid = |msg: String| Err(ShotError::Invalid(msg));
        if self.name.is_empty() || self.name.contains('/') || self.name.contains('\\') {
            return invalid(format!(
                "name must be a plain file stem, got {:?}",
                self.name
            ));
        }
        if self.width == 0 || self.height == 0 {
            return invalid("width/height must be non-zero".into());
        }
        if self.render_distance == 0 || self.render_distance > 32 {
            return invalid("render_distance must be in 1..=32".into());
        }
        if !(0.0..=1.0).contains(&self.brightness) {
            return invalid("brightness must be in 0.0..=1.0".into());
        }
        if !(1.0..=170.0).contains(&self.fov) {
            return invalid("fov must be in 1..=170".into());
        }
        if !(-90.0..=90.0).contains(&self.pitch) {
            return invalid("pitch must be in -90..=90".into());
        }
        Ok(())
    }

    /// Camera eye as the vanilla pipeline stores it: feet position, i.e.
    /// eye minus the standing eye height.
    pub fn feet_position(&self) -> [f64; 3] {
        [self.eye[0], self.eye[1] - EYE_HEIGHT, self.eye[2]]
    }

    /// Yaw in radians for the mcre camera (yaw 0 = north, positive east):
    /// vanilla yaw + 180 degrees.
    pub fn mcre_yaw(&self) -> f32 {
        (self.yaw + 180.0f64).to_radians() as f32
    }

    /// Pitch in radians for the mcre camera (positive looks up): negated
    /// vanilla pitch.
    pub fn mcre_pitch(&self) -> f32 {
        (-self.pitch).to_radians() as f32
    }

    /// Build a spec from mcre-camera values (used by the viewer's
    /// `--emit-spec` helper, which sees mcre-convention angles).
    #[allow(clippy::too_many_arguments)]
    pub fn from_mcre(
        name: String,
        world: PathBuf,
        eye: [f64; 3],
        mcre_yaw_rad: f64,
        mcre_pitch_rad: f64,
        fov_deg: f64,
        brightness: f64,
        time_ticks: i64,
    ) -> Self {
        Self {
            name,
            world,
            eye,
            yaw: mcre_yaw_rad.to_degrees() - 180.0,
            pitch: -mcre_pitch_rad.to_degrees(),
            fov: fov_deg,
            brightness,
            time_ticks,
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            render_distance: DEFAULT_RENDER_DISTANCE,
            clouds: Clouds::default(),
            settle_ms: DEFAULT_SETTLE_MS,
            ao: true,
        }
    }

    /// Default output path for one engine's PNG: `target/shots/<name>.<engine>.png`.
    pub fn out_path(&self, engine: &str) -> PathBuf {
        PathBuf::from("target/shots").join(format!("{}.{}.png", self.name, engine))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> ShotSpec {
        ShotSpec {
            name: "noon".into(),
            world: PathBuf::from("target/gen_world/world"),
            eye: [12.5, 71.62, -40.0],
            yaw: 90.0,
            pitch: -12.5,
            fov: 70.0,
            brightness: 0.5,
            time_ticks: 6000,
            width: 1280,
            height: 720,
            render_distance: 8,
            clouds: Clouds::Off,
            settle_ms: 4000,
            ao: true,
        }
    }

    #[test]
    fn round_trip() {
        let spec = spec();
        let json = serde_json::to_string(&spec).unwrap();
        let parsed: ShotSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(spec, parsed);
    }

    #[test]
    fn defaults_fill_optional_fields() {
        let parsed: ShotSpec =
            serde_json::from_str(r#"{"name":"a","world":"w","eye":[0,0,0],"yaw":0,"pitch":0}"#)
                .unwrap();
        assert_eq!(parsed.fov, DEFAULT_FOV);
        assert_eq!(parsed.brightness, DEFAULT_BRIGHTNESS);
        assert_eq!(parsed.time_ticks, DEFAULT_TIME_TICKS);
        assert_eq!(
            (parsed.width, parsed.height),
            (DEFAULT_WIDTH, DEFAULT_HEIGHT)
        );
        assert_eq!(parsed.clouds, Clouds::Off);
        parsed.validate().unwrap();
    }

    #[test]
    fn angle_conversion_matches_vanilla_view_vector() {
        let mut spec = spec();
        for (yaw, pitch) in [(0.0, 0.0), (90.0, 10.0), (-135.0, -45.0), (200.0, 89.0)] {
            spec.yaw = yaw;
            spec.pitch = pitch;
            let (sy, cy) = (spec.mcre_yaw() as f64).sin_cos();
            let (sp, cp) = (spec.mcre_pitch() as f64).sin_cos();
            let mcre = [sy * cp, sp, -cy * cp];
            let (vy, cvy) = (-yaw.to_radians()).sin_cos();
            let (vp, cvp) = (pitch.to_radians()).sin_cos();
            let vanilla = [vy * cvp, -vp, cvy * cvp];
            for axis in 0..3 {
                assert!(
                    (mcre[axis] - vanilla[axis]).abs() < 1e-6,
                    "yaw {yaw} pitch {pitch}"
                );
            }
        }
    }

    #[test]
    fn feet_below_eye_by_eye_height() {
        let feet = spec().feet_position();
        assert!((feet[1] - (71.62 - EYE_HEIGHT)).abs() < 1e-9);
    }
}
