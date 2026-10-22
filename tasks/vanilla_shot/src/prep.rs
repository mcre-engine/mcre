//! Game-directory and world preparation for a vanilla parity shot.
//!
//! The embedded client is launched with a scratch game directory
//! (`target/shot/client` by default) whose `saves/` folder symlinks the
//! generated world. To get a frozen, HUD-free, deterministic frame we patch:
//!
//! - `options.txt` — FOV/gamma/render distance/clouds, no bob, no pause on
//!   lost focus.
//! - `level.dat` — creative game type.
//! - `data/minecraft/world_clocks.dat` — overworld clock paused at the spec's
//!   `time_ticks`.
//! - `data/minecraft/weather.dat` — permanently clear weather.
//! - `playerdata/<uuid>.dat` — frozen creative-flying player at the spec's
//!   eye position/rotation.

use std::fs;
use std::path::Path;

use mcre_anvil::nbt::Tag;
use mcre_shot::{Clouds, ShotSpec};

/// Fixed offline player identity used for the vanilla shot.
pub const PLAYER_UUID: &str = "00000000-0000-4000-8000-000000000001";
pub const USERNAME: &str = "McreShot";

pub fn prepare(game_dir: &Path, spec: &ShotSpec) -> Result<String, String> {
    let world = &spec.world;
    if !world.join("level.dat").exists() {
        return Err(format!(
            "world {} has no level.dat (run `cargo run -r -p data_gen` + gen_world first)",
            world.display()
        ));
    }
    let save_name = world
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .ok_or_else(|| format!("world path {} has no file name", world.display()))?;

    let data_version = data_version(world);

    link_save(game_dir, world, &save_name)?;
    write_options(game_dir, spec, data_version)?;
    patch_level_dat(world, data_version)?;
    patch_world_clocks(world, spec, data_version)?;
    patch_weather(world, data_version)?;
    write_playerdata(world, spec, data_version)?;

    Ok(save_name)
}

fn data_version(world: &Path) -> i32 {
    let Ok(root) = mcre_anvil::read_level_dat(world) else {
        return 0;
    };
    let value = root
        .get("Data")
        .and_then(|data| data.get("DataVersion"))
        .or_else(|| root.get("DataVersion"));
    match value {
        Some(Tag::Int(version)) => *version,
        _ => 0,
    }
}

fn link_save(game_dir: &Path, world: &Path, save_name: &str) -> Result<(), String> {
    let saves = game_dir.join("saves");
    fs::create_dir_all(&saves).map_err(|err| format!("create {}: {err}", saves.display()))?;
    let link = saves.join(save_name);
    match fs::symlink_metadata(&link) {
        Ok(meta) if meta.file_type().is_symlink() => {
            fs::remove_file(&link).map_err(|err| format!("remove {}: {err}", link.display()))?;
        }
        Ok(_) => {
            return Err(format!(
                "{} exists and is not a symlink; delete it manually",
                link.display()
            ));
        }
        Err(_) => {}
    }
    let target = fs::canonicalize(world)
        .map_err(|err| format!("canonicalize {}: {err}", world.display()))?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, &link)
        .map_err(|err| format!("symlink {}: {err}", link.display()))?;
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&target, &link)
        .map_err(|err| format!("symlink {}: {err}", link.display()))?;
    Ok(())
}

fn write_options(game_dir: &Path, spec: &ShotSpec, data_version: i32) -> Result<(), String> {
    let fov = (spec.fov - 70.0) / 40.0;
    let clouds = match spec.clouds {
        Clouds::Off => "\"false\"",
        Clouds::Fast => "\"fast\"",
        Clouds::Fancy => "\"true\"",
    };
    let content = format!(
        "version:{data_version}\n\
         ao:true\n\
         onboardAccessibility:false\n\
         fov:{fov}\n\
         fovEffectScale:1.0\n\
         fovEffectHalfStrength:1.0\n\
         gamma:{gamma}\n\
         guiScale:1\n\
         renderDistance:{render_distance}\n\
         simulationDistance:{render_distance}\n\
         renderClouds:{clouds}\n\
         bobView:false\n\
         pauseOnLostFocus:false\n\
         fullscreen:false\n\
         music:0\n",
        gamma = spec.brightness,
        render_distance = spec.render_distance,
    );
    let path = game_dir.join("options.txt");
    fs::write(&path, content).map_err(|err| format!("write {}: {err}", path.display()))
}

fn patch_level_dat(world: &Path, data_version: i32) -> Result<(), String> {
    let mut root =
        mcre_anvil::read_level_dat(world).map_err(|err| format!("read level.dat: {err}"))?;
    if !matches!(root.get("Data"), Some(Tag::Compound(_))) {
        return Err("level.dat has no Data compound".into());
    }
    let data = root.get_mut("Data").expect("checked above");
    data.set("GameType", Tag::Int(1));
    if data_version != 0 {
        data.set("DataVersion", Tag::Int(data_version));
    }
    mcre_anvil::write_level_dat(world, &root).map_err(|err| format!("write level.dat: {err}"))
}

/// Freeze the overworld day clock at `spec.time_ticks`.
///
/// `world_clocks.dat` holds a `SavedData` codec map keyed by dimension; the
/// per-dimension `ClockState` has `total_ticks` (required) plus optional
/// `partial_tick`/`rate`/`paused`. Pausing keeps the value constant on both
/// server and client instead of drifting with real time.
fn patch_world_clocks(world: &Path, spec: &ShotSpec, data_version: i32) -> Result<(), String> {
    let path = world
        .join("data")
        .join("minecraft")
        .join("world_clocks.dat");
    let overworld = Tag::Compound(vec![
        ("total_ticks".to_string(), Tag::Long(spec.time_ticks)),
        ("partial_tick".to_string(), Tag::Float(0.0)),
        ("rate".to_string(), Tag::Float(1.0)),
        ("paused".to_string(), Tag::Byte(1)),
    ]);
    let root = Tag::Compound(vec![
        (
            "data".to_string(),
            Tag::Compound(vec![("minecraft:overworld".to_string(), overworld)]),
        ),
        ("DataVersion".to_string(), Tag::Int(data_version)),
    ]);
    mcre_anvil::write_gzip_nbt(&path, &root)
        .map_err(|err| format!("write {}: {err}", path.display()))
}

/// Keep weather permanently clear (`clear_weather_time` counts down from a
/// huge value; rain/thunder timers are zero and both flags false).
fn patch_weather(world: &Path, data_version: i32) -> Result<(), String> {
    let path = world.join("data").join("minecraft").join("weather.dat");
    let mut entries: Vec<(String, Tag)> = match mcre_anvil::read_gzip_nbt(&path) {
        Ok(Tag::Compound(root)) => match root.iter().find(|(key, _)| key == "data") {
            Some((_, Tag::Compound(data))) => data.clone(),
            _ => Vec::new(),
        },
        Ok(_) => return Err(format!("{}: unexpected root tag", path.display())),
        Err(_) => Vec::new(),
    };
    set_entry(&mut entries, "clear_weather_time", Tag::Int(1_000_000));
    set_entry(&mut entries, "rain_time", Tag::Int(0));
    set_entry(&mut entries, "thunder_time", Tag::Int(0));
    set_entry(&mut entries, "raining", Tag::Byte(0));
    set_entry(&mut entries, "thundering", Tag::Byte(0));

    let root = Tag::Compound(vec![
        ("data".to_string(), Tag::Compound(entries)),
        ("DataVersion".to_string(), Tag::Int(data_version)),
    ]);
    mcre_anvil::write_gzip_nbt(&path, &root)
        .map_err(|err| format!("write {}: {err}", path.display()))
}

/// Write a minimal playerdata file placing a frozen creative-flying player at
/// the spec's eye position. The vanilla loader fills in everything else.
fn write_playerdata(world: &Path, spec: &ShotSpec, data_version: i32) -> Result<(), String> {
    let feet = spec.feet_position();
    let root = Tag::Compound(vec![
        (
            "Pos".to_string(),
            Tag::List(vec![
                Tag::Double(feet[0]),
                Tag::Double(feet[1]),
                Tag::Double(feet[2]),
            ]),
        ),
        (
            "Rotation".to_string(),
            Tag::List(vec![
                Tag::Float(spec.yaw as f32),
                Tag::Float(spec.pitch as f32),
            ]),
        ),
        (
            "Motion".to_string(),
            Tag::List(vec![Tag::Double(0.0), Tag::Double(0.0), Tag::Double(0.0)]),
        ),
        ("OnGround".to_string(), Tag::Byte(0)),
        (
            "Dimension".to_string(),
            Tag::String("minecraft:overworld".to_string()),
        ),
        ("playerGameType".to_string(), Tag::Int(1)),
        (
            "abilities".to_string(),
            Tag::Compound(vec![
                ("flying".to_string(), Tag::Byte(1)),
                ("mayfly".to_string(), Tag::Byte(1)),
                ("invulnerable".to_string(), Tag::Byte(1)),
                ("instabuild".to_string(), Tag::Byte(1)),
                ("mayBuild".to_string(), Tag::Byte(1)),
                ("flySpeed".to_string(), Tag::Float(0.05)),
                ("walkSpeed".to_string(), Tag::Float(0.1)),
            ]),
        ),
        ("DataVersion".to_string(), Tag::Int(data_version)),
    ]);
    let path = world
        .join("playerdata")
        .join(format!("{}.dat", PLAYER_UUID));
    mcre_anvil::write_gzip_nbt(&path, &root)
        .map_err(|err| format!("write {}: {err}", path.display()))
}

fn set_entry(entries: &mut Vec<(String, Tag)>, key: &str, value: Tag) {
    match entries.iter_mut().find(|(name, _)| name == key) {
        Some(slot) => slot.1 = value,
        None => entries.push((key.to_string(), value)),
    }
}
