//! Capture a vanilla Minecraft screenshot driven by a shared shot spec.
//!
//! The client runs on the process main thread (required by SDL3/AppKit on
//! macOS) while a controller thread drives it through JNI: wait for the
//! world, verify/fix the camera, hide the HUD, grab the frame, shut down.
//!
//! Usage: `cargo run -r -p vanilla_shot -- --spec <spec.json> [--out <png>] [--game-dir <dir>]`

mod assets;
mod prep;

use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

use mcre_shot::ShotSpec;

const WORLD_TIMEOUT: Duration = Duration::from_secs(600);
const GRAB_TIMEOUT: Duration = Duration::from_secs(60);
const HIDE_SETTLE: Duration = Duration::from_millis(1200);
const FIX_SETTLE: Duration = Duration::from_millis(1500);
const WINDOW_SETTLE: Duration = Duration::from_millis(500);
const CAMERA_EPSILON_POS: f64 = 1e-3;
const CAMERA_EPSILON_DEG: f64 = 0.05;
const CAMERA_EPSILON_FOV: f64 = 0.05;

struct Options {
    spec: PathBuf,
    out: Option<PathBuf>,
    game_dir: Option<PathBuf>,
    help: bool,
}

impl Options {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut spec = None;
        let mut out = None;
        let mut game_dir = None;
        let mut help = false;

        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--spec" => {
                    index += 1;
                    let value = args.get(index).ok_or("--spec needs a value")?;
                    spec = Some(PathBuf::from(value));
                }
                "--out" => {
                    index += 1;
                    let value = args.get(index).ok_or("--out needs a value")?;
                    out = Some(PathBuf::from(value));
                }
                "--game-dir" => {
                    index += 1;
                    let value = args.get(index).ok_or("--game-dir needs a value")?;
                    game_dir = Some(PathBuf::from(value));
                }
                "--help" | "-h" => help = true,
                other => return Err(format!("unknown argument: {other}")),
            }
            index += 1;
        }

        Ok(Self {
            spec: spec.ok_or("--spec is required")?,
            out,
            game_dir,
            help,
        })
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let options = match Options::parse(&args) {
        Ok(options) => options,
        Err(err) => {
            eprintln!("vanilla_shot: {err}");
            return ExitCode::from(2);
        }
    };
    if options.help {
        println!("usage: vanilla_shot --spec <spec.json> [--out <png>] [--game-dir <dir>]");
        return ExitCode::SUCCESS;
    }

    match run(&options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("vanilla_shot: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(options: &Options) -> Result<(), String> {
    let spec = ShotSpec::load(&options.spec).map_err(|err| err.to_string())?;
    let out = options
        .out
        .clone()
        .unwrap_or_else(|| spec.out_path("vanilla"));
    let game_dir = options
        .game_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from("target/shot/client"));
    fs::create_dir_all(&game_dir).map_err(|err| format!("create {}: {err}", game_dir.display()))?;
    let game_dir =
        fs::canonicalize(&game_dir).map_err(|err| format!("canonicalize game dir: {err}"))?;

    let save_name = prep::prepare(&game_dir, &spec)?;
    let shot_name = format!("{}.png", spec.name);
    eprintln!(
        "vanilla_shot: prepared {} (save {save_name}), capturing {}x{}",
        game_dir.display(),
        spec.width,
        spec.height
    );

    let client_args = client_args(&game_dir, &spec, &save_name);

    let runtime = tokio::runtime::Runtime::new().map_err(|err| format!("tokio: {err}"))?;
    runtime.block_on(assets::ensure())?;
    let jvm = runtime.block_on(mcje::init());

    let control = mcje::client::ClientControl::new(&jvm);
    let controller = {
        let control = control.clone();
        let spec = spec.clone();
        let out = out.clone();
        let game_dir = game_dir.clone();
        thread::spawn(move || {
            let result = drive(&control, &spec, &game_dir, &out, &shot_name);
            let _ = control.stop();
            result
        })
    };

    let client_result = mcje::client::run_client_blocking(&jvm, &client_args);
    let controller_result = controller
        .join()
        .unwrap_or_else(|_| Err("controller thread panicked".into()));

    controller_result?;
    client_result.map_err(|err| format!("client failed: {err}"))?;
    eprintln!("vanilla_shot: wrote {}", out.display());
    Ok(())
}

fn client_args(game_dir: &Path, spec: &ShotSpec, save_name: &str) -> Vec<String> {
    let width = spec.width.to_string();
    let height = spec.height.to_string();
    [
        "--username",
        prep::USERNAME,
        "--uuid",
        prep::PLAYER_UUID,
        "--accessToken",
        "0",
        "--version",
        mcje::VERSION,
        "--versionType",
        "snapshot",
        "--gameDir",
        game_dir.to_str().expect("game dir utf-8"),
        "--assetsDir",
        mcje::ASSETS_DIR,
        "--assetIndex",
        mcje::ASSET_INDEX,
        "--width",
        &width,
        "--height",
        &height,
        "--quickPlaySingleplayer",
        save_name,
    ]
    .iter()
    .map(|value| (*value).to_string())
    .collect()
}

fn drive(
    control: &mcje::client::ClientControl,
    spec: &ShotSpec,
    game_dir: &Path,
    out: &Path,
    shot_name: &str,
) -> Result<(), String> {
    eprintln!("vanilla_shot: waiting for world");
    control.wait_for_world(WORLD_TIMEOUT)?;

    // Let the loading overlay finish and the world settle before verifying.
    thread::sleep(Duration::from_millis(spec.settle_ms));
    check_step_error(control)?;

    eprintln!("vanilla_shot: verifying camera");
    verify_and_fix_camera(control, spec)?;
    eprintln!("vanilla_shot: verifying framebuffer");
    verify_framebuffer(control, spec)?;
    eprintln!("vanilla_shot: verifying clock");
    verify_clock(control, spec)?;

    eprintln!("vanilla_shot: saturating water vision");
    control.schedule_water_saturate()?;
    check_step_error(control)?;

    eprintln!("vanilla_shot: hiding HUD");
    control.schedule_hide()?;
    thread::sleep(HIDE_SETTLE);
    check_step_error(control)?;
    eprintln!(
        "vanilla_shot: water vision {}",
        control.player_state()?.water_vision
    );

    eprintln!("vanilla_shot: freezing world, animations, and sound");
    control.schedule_freeze()?;
    thread::sleep(FIX_SETTLE);
    check_step_error(control)?;

    let shot_path = game_dir.join("screenshots").join(shot_name);
    let _ = fs::remove_file(&shot_path);
    control.schedule_grab(shot_name)?;
    wait_for_screenshot(control, &shot_path)?;
    let (water_vision, animation_ticks) = control.shot_state()?;
    let state_path = out.with_extension("state.json");
    if let Some(parent) = state_path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("create {}: {err}", parent.display()))?;
    }
    fs::write(
        &state_path,
        format!("{{\"water_vision\":{water_vision},\"animation_ticks\":{animation_ticks}}}\n"),
    )
    .map_err(|err| format!("write {}: {err}", state_path.display()))?;
    eprintln!("vanilla_shot: water_vision={water_vision}, animation_ticks={animation_ticks}");

    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("create {}: {err}", parent.display()))?;
    }
    fs::copy(&shot_path, out)
        .map_err(|err| format!("copy {} -> {}: {err}", shot_path.display(), out.display()))?;
    let (width, height) = png_dimensions(out)?;
    let (want_width, want_height) = (spec.width, spec.height);
    if width != want_width || height != want_height {
        return Err(format!(
            "screenshot is {width}x{height}, want {want_width}x{want_height} \
             (display scaling mismatch?)"
        ));
    }
    Ok(())
}

/// Verify the client camera matches the spec, correcting it through a
/// server-side teleport when it does not (the player may spawn at a slightly
/// different eye height or snap its rotation on join).
fn verify_and_fix_camera(
    control: &mcje::client::ClientControl,
    spec: &ShotSpec,
) -> Result<(), String> {
    let expected_eye = spec.eye;
    let expected_yaw = spec.yaw;
    let expected_pitch = spec.pitch;

    for _attempt in 0..3 {
        let state = control.player_state()?;
        let eye = state.eye;
        let position_ok = (eye[0] - expected_eye[0]).abs() < CAMERA_EPSILON_POS
            && (eye[1] - expected_eye[1]).abs() < CAMERA_EPSILON_POS
            && (eye[2] - expected_eye[2]).abs() < CAMERA_EPSILON_POS;
        let yaw_ok = angle_delta(state.yaw as f64, expected_yaw) < CAMERA_EPSILON_DEG;
        let pitch_ok = angle_delta(state.pitch as f64, expected_pitch) < CAMERA_EPSILON_DEG;
        let fov_ok = (f64::from(state.fov) - spec.fov).abs() < CAMERA_EPSILON_FOV;
        if position_ok && yaw_ok && pitch_ok && fov_ok {
            eprintln!(
                "vanilla_shot: camera verified at eye {eye:?} (eye height {}, fov {})",
                state.eye_height, state.fov
            );
            return Ok(());
        }
        if position_ok && yaw_ok && pitch_ok {
            return Err(format!(
                "client fov is {}, want {} (a camera modifier is active)",
                state.fov, spec.fov
            ));
        }
        eprintln!(
            "vanilla_shot: camera mismatch (eye {eye:?} yaw {} pitch {}), teleporting",
            state.yaw, state.pitch
        );
        let feet = [
            expected_eye[0],
            expected_eye[1] - f64::from(state.eye_height),
            expected_eye[2],
        ];
        control.schedule_fix(feet, expected_yaw as f32, expected_pitch as f32)?;
        thread::sleep(FIX_SETTLE);
        check_step_error(control)?;
    }
    Err("camera did not converge after teleport".into())
}

/// The client's framebuffer must equal the spec resolution; on high-DPI
/// displays the window scale multiplies the requested size, so nudge the
/// logical window size until the framebuffer lands on the target.
fn verify_framebuffer(
    control: &mcje::client::ClientControl,
    spec: &ShotSpec,
) -> Result<(), String> {
    let target = (spec.width as i32, spec.height as i32);
    let mut scale: Option<(f64, f64)> = None;
    for _attempt in 0..5 {
        let (width, height) = control.framebuffer_size()?;
        if (width, height) == target {
            return Ok(());
        }
        eprintln!(
            "vanilla_shot: framebuffer is {width}x{height}, want {}x{}",
            target.0, target.1
        );
        let (scale_x, scale_y) = *scale.get_or_insert_with(|| {
            (
                f64::from(width) / f64::from(spec.width as i32),
                f64::from(height) / f64::from(spec.height as i32),
            )
        });
        let logical = (
            (f64::from(target.0) / scale_x).round() as i32,
            (f64::from(target.1) / scale_y).round() as i32,
        );
        control.set_windowed(logical.0, logical.1)?;
        thread::sleep(WINDOW_SETTLE);
    }
    Err(format!(
        "framebuffer never reached {}x{}; capture would not line up",
        target.0, target.1
    ))
}

/// The world clock must have been frozen at the spec's time.
fn verify_clock(control: &mcje::client::ClientControl, spec: &ShotSpec) -> Result<(), String> {
    let time = control.overworld_clock_time()?;
    if time != spec.time_ticks {
        return Err(format!(
            "client clock is {time}, want {} (world_clocks patch did not apply)",
            spec.time_ticks
        ));
    }
    eprintln!("vanilla_shot: clock frozen at {time}");
    Ok(())
}

fn check_step_error(control: &mcje::client::ClientControl) -> Result<(), String> {
    match control.shot_error()? {
        Some(err) => Err(format!("client step failed: {err}")),
        None => Ok(()),
    }
}

/// Wait until the screenshot exists with a stable size (the PNG is written
/// asynchronously on the client's IO pool).
fn wait_for_screenshot(control: &mcje::client::ClientControl, path: &Path) -> Result<(), String> {
    let deadline = Instant::now() + GRAB_TIMEOUT;
    let mut last_size = 0u64;
    let mut stable_since: Option<Instant> = None;
    loop {
        if let Ok(meta) = fs::metadata(path) {
            let size = meta.len();
            if size > 0 && size == last_size {
                match stable_since {
                    Some(since) if since.elapsed() >= Duration::from_millis(400) => {
                        return Ok(());
                    }
                    Some(_) => {}
                    None => stable_since = Some(Instant::now()),
                }
            } else {
                stable_since = None;
                last_size = size;
            }
        }
        if let Some(err) = control.shot_error()? {
            return Err(format!("client step failed: {err}"));
        }
        if !control.render_thread_alive() {
            return Err("render thread exited before the screenshot was written".into());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "screenshot {} not written within {GRAB_TIMEOUT:?}",
                path.display()
            ));
        }
        thread::sleep(Duration::from_millis(150));
    }
}

fn png_dimensions(path: &Path) -> Result<(u32, u32), String> {
    let mut header = [0u8; 24];
    let mut file = fs::File::open(path).map_err(|err| format!("open {}: {err}", path.display()))?;
    file.read_exact(&mut header)
        .map_err(|err| format!("read {}: {err}", path.display()))?;
    if &header[0..8] != b"\x89PNG\r\n\x1a\n" {
        return Err(format!("{} is not a PNG", path.display()));
    }
    let width = u32::from_be_bytes(header[16..20].try_into().expect("png width"));
    let height = u32::from_be_bytes(header[20..24].try_into().expect("png height"));
    Ok((width, height))
}

fn angle_delta(a: f64, b: f64) -> f64 {
    let mut delta = (a - b) % 360.0;
    if delta > 180.0 {
        delta -= 360.0;
    } else if delta < -180.0 {
        delta += 360.0;
    }
    delta.abs()
}
