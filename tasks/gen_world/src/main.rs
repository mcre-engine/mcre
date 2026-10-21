mod pregen;
mod rcon;

use std::collections::hash_map::DefaultHasher;
use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{Read, Seek, SeekFrom};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use rcon::RconClient;

const DEFAULT_SEED: &str = "0";
const DEFAULT_RADIUS: u32 = 512;
const DEFAULT_WINDOW_TILES: usize = 8;
const RCON_CONNECT_TIMEOUT: Duration = Duration::from_secs(600);
const STOP_TIMEOUT: Duration = Duration::from_secs(120);
const QUIESCENCE_CHECK: Duration = Duration::from_millis(1500);
const QUIESCENCE_TIMEOUT: Duration = Duration::from_secs(60);

struct Options {
    seed: String,
    radius: u32,
    window: usize,
    help: bool,
}

impl Options {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut seed = None;
        let mut radius = None;
        let mut window = DEFAULT_WINDOW_TILES;
        let mut help = false;
        let mut positional = 0;

        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "-h" | "--help" => help = true,
                "--seed" => {
                    let value = iter.next().ok_or("--seed needs a value")?;
                    seed = Some(value.clone());
                }
                "--radius" => {
                    let value = iter.next().ok_or("--radius needs a value")?;
                    radius = Some(
                        value
                            .parse()
                            .map_err(|_| format!("invalid radius: {value}"))?,
                    );
                }
                "--window" => {
                    let value = iter.next().ok_or("--window needs a value")?;
                    window = value
                        .parse()
                        .map_err(|_| format!("invalid window: {value}"))?;
                    if window == 0 {
                        return Err("window must be at least 1".into());
                    }
                }
                other if other.starts_with('-') => {
                    return Err(format!("unknown flag: {other}"));
                }
                other => {
                    match positional {
                        0 => seed = Some(other.to_string()),
                        1 => {
                            radius = Some(
                                other
                                    .parse()
                                    .map_err(|_| format!("invalid radius: {other}"))?,
                            );
                        }
                        _ => return Err(format!("unexpected argument: {other}")),
                    }
                    positional += 1;
                }
            }
        }

        Ok(Self {
            seed: seed.unwrap_or_else(|| DEFAULT_SEED.to_string()),
            radius: radius.unwrap_or(DEFAULT_RADIUS),
            window,
            help,
        })
    }
}

fn usage() {
    let diameter = DEFAULT_RADIUS * 2;
    eprintln!(
        "usage: gen_world [--seed SEED] [--radius CHUNKS] [--window TILES]\n\
         \n\
         Generates a vanilla Minecraft world under target/gen_world/world.\n\
         Defaults: seed {DEFAULT_SEED}, radius {DEFAULT_RADIUS} chunks\n\
         (a {diameter}x{diameter} chunk square), window {DEFAULT_WINDOW_TILES} tiles.\n\
         Note: by running this you accept the Minecraft EULA (https://aka.ms/MinecraftEULA)."
    );
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let options = match Options::parse(&args) {
        Ok(options) => options,
        Err(err) => {
            eprintln!("gen_world: {err}");
            usage();
            std::process::exit(2);
        }
    };
    if options.help {
        usage();
        return;
    }

    eprintln!(
        "gen_world: generating vanilla world (seed {}, radius {} chunks); \
         running this tool means you accept the Minecraft EULA (https://aka.ms/MinecraftEULA)",
        options.seed, options.radius
    );

    let out_root = match prepare_out_root() {
        Ok(path) => path,
        Err(err) => {
            eprintln!("gen_world: {err}");
            std::process::exit(1);
        }
    };
    let world = out_root.join("world");
    let logs = out_root.join("logs").join("latest.log");

    let game_port = match free_port() {
        Some(port) => port,
        None => die(None, &logs, "could not pick a game port"),
    };
    let rcon_port = match free_port() {
        Some(port) if port != game_port => port,
        Some(_) => match free_port() {
            Some(port) => port,
            None => die(None, &logs, "could not pick an RCON port"),
        },
        None => die(None, &logs, "could not pick an RCON port"),
    };
    let password = generate_password();

    if let Err(err) = write_server_properties(&out_root, &options, game_port, rcon_port, &password)
    {
        die(
            None,
            &logs,
            &format!("could not write server.properties: {err}"),
        );
    }
    if let Err(err) = fs::write(out_root.join("eula.txt"), "eula=true\n") {
        die(None, &logs, &format!("could not write eula.txt: {err}"));
    }

    if let Err(err) = env::set_current_dir(&out_root) {
        die(
            None,
            &logs,
            &format!("could not enter {}: {err}", out_root.display()),
        );
    }

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(err) => die(
            None,
            &logs,
            &format!("could not start tokio runtime: {err}"),
        ),
    };
    let jvm = runtime.block_on(mcje::init());
    let handle = mcje::spawn_dedicated_server(jvm, vec!["--nogui".to_string()]);

    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, rcon_port));
    let mut client = match connect_rcon(&handle, addr, &password) {
        Ok(client) => client,
        Err(message) => die(Some(&handle), &logs, &message),
    };
    let _ = client.run("forceload remove all");

    let rect = pregen::ChunkRect::from_radius(options.radius);
    let tiles = pregen::tiles_for(&rect);
    let total = rect.total_chunks();
    let already = pregen::prescan(&world, &tiles).full_chunks();
    if already > 0 {
        let tile_word = if tiles.len() == 1 { "tile" } else { "tiles" };
        eprintln!(
            "gen_world: resuming — {already}/{total} chunks already full across {} {tile_word}",
            tiles.len()
        );
    }

    let is_aborted = || handle.has_failed();
    match pregen::run_pregen(
        &world,
        &rect,
        &tiles,
        &mut client,
        options.window,
        &is_aborted,
        |progress| {
            eprint!(
                "\rgen_world: {}/{} chunks ({:.1}/s, {} tiles active) ETA {}",
                progress.done_chunks,
                progress.total_chunks,
                progress.rate,
                progress.active_tiles,
                progress.eta_text(),
            );
        },
    ) {
        Ok(summary) => {
            eprintln!(
                "\ngen_world: generation complete — {} chunks in {:.1}s",
                summary.total_chunks,
                summary.elapsed.as_secs_f64()
            );
        }
        Err(err) => die(Some(&handle), &logs, &format!("pregen failed: {err}")),
    }

    match pregen::verify(&world, &tiles, &mut client, &is_aborted) {
        Ok(summary) => {
            eprintln!(
                "gen_world: verified {} chunks in {:.1}s",
                summary.checked_chunks,
                summary.elapsed.as_secs_f64()
            );
        }
        Err(err) => die(Some(&handle), &logs, &format!("verification failed: {err}")),
    }

    stop_server(&handle, &mut client, &world);
    eprintln!("gen_world: world ready at {}", world.display());
    handle.shutdown();
    std::process::exit(0);
}

fn prepare_out_root() -> Result<PathBuf, String> {
    let cwd = env::current_dir().map_err(|err| format!("cannot read cwd: {err}"))?;
    let out_root = cwd.join("target").join("gen_world");
    fs::create_dir_all(&out_root)
        .map_err(|err| format!("cannot create {}: {err}", out_root.display()))?;
    out_root
        .canonicalize()
        .map_err(|err| format!("cannot resolve {}: {err}", out_root.display()))
}

fn free_port() -> Option<u16> {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .ok()?
        .local_addr()
        .ok()
        .map(|addr| addr.port())
}

fn generate_password() -> String {
    let mut hasher = DefaultHasher::new();
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default()
        .hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn write_server_properties(
    out_root: &Path,
    options: &Options,
    game_port: u16,
    rcon_port: u16,
    password: &str,
) -> std::io::Result<()> {
    let content = format!(
        "level-name=world\n\
         level-seed={}\n\
         enable-rcon=true\n\
         rcon.port={rcon_port}\n\
         rcon.password={password}\n\
         server-ip=127.0.0.1\n\
         server-port={game_port}\n\
         pause-when-empty-seconds=0\n\
         max-tick-time=-1\n\
         view-distance=8\n\
         sync-chunk-writes=true\n",
        options.seed
    );
    fs::write(out_root.join("server.properties"), content)
}

fn connect_rcon(
    handle: &mcje::ServerHandle,
    addr: SocketAddr,
    password: &str,
) -> Result<RconClient, String> {
    let started = Instant::now();
    while started.elapsed() < RCON_CONNECT_TIMEOUT {
        if handle.has_failed() {
            return Err("server failed during startup".into());
        }
        match RconClient::connect(addr, password) {
            Ok(client) => return Ok(client),
            Err(_) => std::thread::sleep(Duration::from_millis(500)),
        }
    }
    Err("timed out waiting for RCON".into())
}

fn stop_server(handle: &mcje::ServerHandle, client: &mut RconClient, world: &Path) {
    if let Err(err) = client.run_with_deadline("save-all flush", Duration::from_secs(600)) {
        eprintln!("gen_world: save-all flush failed: {err}");
    }
    handle.mark_stopping();
    let _ = client.run_with_deadline("stop", Duration::from_secs(30));

    let addr = client.addr();
    let deadline = Instant::now() + STOP_TIMEOUT;
    while Instant::now() < deadline {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_err() {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    wait_quiescence(world);
}

fn wait_quiescence(world: &Path) {
    let deadline = Instant::now() + QUIESCENCE_TIMEOUT;
    let mut previous = None;
    while Instant::now() < deadline {
        let total = dir_size(world);
        match previous {
            Some(prev) if prev == total => return,
            _ => {}
        }
        previous = Some(total);
        std::thread::sleep(QUIESCENCE_CHECK);
    }
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    for entry in entries.flatten() {
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_dir() {
            total += dir_size(&entry.path());
        } else {
            total += metadata.len();
        }
    }
    total
}

fn die(handle: Option<&mcje::ServerHandle>, logs: &Path, message: &str) -> ! {
    eprintln!("\ngen_world: {message}");
    tail_log(logs);
    if let Some(handle) = handle {
        handle.mark_stopping();
    }
    std::process::exit(1);
}

fn tail_log(logs: &Path) {
    let Ok(mut file) = fs::File::open(logs) else {
        return;
    };
    let length = file.metadata().map(|m| m.len()).unwrap_or(0);
    let start = length.saturating_sub(8192);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return;
    }
    let mut content = String::new();
    if file.read_to_string(&mut content).is_err() {
        return;
    }
    eprintln!("gen_world: last server log lines:");
    let lines: Vec<&str> = content.lines().collect();
    for line in &lines[lines.len().saturating_sub(80)..] {
        eprintln!("  {line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_defaults_and_flags() {
        let options = Options::parse(&[]).unwrap();
        assert_eq!(options.seed, DEFAULT_SEED);
        assert_eq!(options.radius, DEFAULT_RADIUS);
        assert_eq!(options.window, DEFAULT_WINDOW_TILES);

        let options = Options::parse(&[
            "--seed".into(),
            "42".into(),
            "--radius".into(),
            "128".into(),
            "--window".into(),
            "2".into(),
        ])
        .unwrap();
        assert_eq!(options.seed, "42");
        assert_eq!(options.radius, 128);
        assert_eq!(options.window, 2);

        let options =
            Options::parse(&["99".into(), "64".into(), "--window".into(), "2".into()]).unwrap();
        assert_eq!(options.seed, "99");
        assert_eq!(options.radius, 64);
    }

    #[test]
    fn rejects_bad_arguments() {
        assert!(Options::parse(&["--nope".into()]).is_err());
        assert!(Options::parse(&["--window".into()]).is_err());
        assert!(Options::parse(&["--window".into(), "0".into()]).is_err());
        assert!(Options::parse(&["1".into(), "2".into(), "3".into()]).is_err());
    }

    #[test]
    fn password_is_hex_token() {
        let password = generate_password();
        assert_eq!(password.len(), 16);
        assert!(password.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
