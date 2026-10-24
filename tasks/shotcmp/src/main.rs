//! Compare two PNG screenshots and write an amplified grayscale diff image.
//!
//! Usage: `shotcmp <a.png> <b.png> <diff.png> [--amplify N] [--max-mae X]`
//!
//! Stats printed: mean absolute error (all channels, 0-255) and the share of
//! pixels whose worst channel deviates by more than 4 / 16. With `--max-mae`
//! the exit code is 1 when the MAE exceeds the threshold (for scripted
//! parity checks).

use std::env;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;
use std::process::ExitCode;

struct Options {
    a: std::path::PathBuf,
    b: std::path::PathBuf,
    diff: std::path::PathBuf,
    amplify: u32,
    max_mae: Option<f64>,
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut positional = Vec::new();
    let mut amplify = 8;
    let mut max_mae = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--amplify" => {
                index += 1;
                amplify = args
                    .get(index)
                    .ok_or("--amplify needs a value")?
                    .parse()
                    .map_err(|err| format!("--amplify: {err}"))?;
            }
            "--max-mae" => {
                index += 1;
                max_mae = Some(
                    args.get(index)
                        .ok_or("--max-mae needs a value")?
                        .parse()
                        .map_err(|err| format!("--max-mae: {err}"))?,
                );
            }
            "--help" | "-h" => return Err(String::new()),
            other if other.starts_with('-') => {
                return Err(format!("unknown flag: {other}"));
            }
            other => positional.push(other.to_string()),
        }
        index += 1;
    }
    if positional.len() != 3 {
        return Err("usage: shotcmp <a.png> <b.png> <diff.png> [--amplify N] [--max-mae X]".into());
    }
    Ok(Options {
        a: positional[0].clone().into(),
        b: positional[1].clone().into(),
        diff: positional[2].clone().into(),
        amplify,
        max_mae,
    })
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let options = match parse(&args) {
        Ok(options) => options,
        Err(err) if err.is_empty() => {
            println!("usage: shotcmp <a.png> <b.png> <diff.png> [--amplify N] [--max-mae X]");
            return ExitCode::SUCCESS;
        }
        Err(err) => {
            eprintln!("shotcmp: {err}");
            return ExitCode::from(2);
        }
    };

    match compare(&options) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(err) => {
            eprintln!("shotcmp: {err}");
            ExitCode::from(2)
        }
    }
}

fn compare(options: &Options) -> Result<bool, String> {
    let (a, a_width, a_height) = decode(&options.a)?;
    let (b, b_width, b_height) = decode(&options.b)?;
    if (a_width, a_height) != (b_width, b_height) {
        return Err(format!(
            "size mismatch: {} is {a_width}x{a_height}, {} is {b_width}x{b_height}",
            options.a.display(),
            options.b.display()
        ));
    }

    let pixels = (a_width * a_height) as usize;
    let mut total_abs = 0u64;
    let mut over_4 = 0u64;
    let mut over_16 = 0u64;
    let mut diff_gray = vec![0u8; pixels];

    for (pixel, gray) in diff_gray.iter_mut().enumerate() {
        let mut worst = 0u8;
        for channel in 0..4 {
            let index = pixel * 4 + channel;
            let delta = a[index].abs_diff(b[index]);
            total_abs += u64::from(delta);
            worst = worst.max(delta);
        }
        if worst > 4 {
            over_4 += 1;
        }
        if worst > 16 {
            over_16 += 1;
        }
        *gray = (u32::from(worst) * options.amplify).min(255) as u8;
    }

    let channels = (pixels * 4) as f64;
    let mae = total_abs as f64 / channels;
    println!(
        "mae {:.4}  >4: {:.3}%  >16: {:.3}%  ({a_width}x{a_height})",
        mae,
        over_4 as f64 * 100.0 / pixels as f64,
        over_16 as f64 * 100.0 / pixels as f64,
    );

    write_gray_png(&options.diff, &diff_gray, a_width, a_height)?;

    Ok(match options.max_mae {
        Some(limit) => mae <= limit,
        None => true,
    })
}

fn decode(path: &Path) -> Result<(Vec<u8>, u32, u32), String> {
    let file = File::open(path).map_err(|err| format!("open {}: {err}", path.display()))?;
    let mut decoder = png::Decoder::new(BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|err| format!("decode {}: {err}", path.display()))?;
    let mut buffer = vec![0u8; reader.output_buffer_size().unwrap_or(0)];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|err| format!("decode {}: {err}", path.display()))?;
    let width = info.width;
    let height = info.height;

    let rgba = match info.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::Rgb => {
            let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
            for pixel in buffer.as_chunks::<3>().0 {
                rgba.extend_from_slice(pixel);
                rgba.push(255);
            }
            rgba
        }
        png::ColorType::Grayscale => {
            let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
            for value in buffer {
                rgba.extend_from_slice(&[value, value, value, 255]);
            }
            rgba
        }
        png::ColorType::GrayscaleAlpha => {
            let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
            for pixel in buffer.as_chunks::<2>().0 {
                rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]);
            }
            rgba
        }
        png::ColorType::Indexed => {
            return Err(format!(
                "{}: palette PNGs are not supported",
                path.display()
            ));
        }
    };
    Ok((rgba, width, height))
}

fn write_gray_png(path: &Path, gray: &[u8], width: u32, height: u32) -> Result<(), String> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("create {}: {err}", parent.display()))?;
    }
    let file = File::create(path).map_err(|err| format!("create {}: {err}", path.display()))?;
    let mut writer = BufWriter::new(file);
    let mut encoder = png::Encoder::new(&mut writer, width, height);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    let mut stream = encoder
        .write_header()
        .map_err(|err| format!("write {}: {err}", path.display()))?;
    stream
        .write_image_data(gray)
        .map_err(|err| format!("write {}: {err}", path.display()))
}
