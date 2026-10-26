use std::io::Cursor;

use mcre_fs::path::FsPath;

pub const COLORMAP_NAMES: [&str; 3] = ["grass", "foliage", "dry_foliage"];
const COLORMAP_WIDTH: u32 = 256;
const COLORMAP_HEIGHT: u32 = 256;

pub const GRASS_DEFAULT: [u8; 3] = [255, 0, 255];
pub const FOLIAGE_DEFAULT: [u8; 3] = [72, 181, 24];
pub const DRY_FOLIAGE_DEFAULT: [u8; 3] = [92, 60, 50];

#[derive(Debug, Clone)]
pub struct Colormaps {
    pub grass: Box<[u8]>,
    pub foliage: Box<[u8]>,
    pub dry_foliage: Box<[u8]>,
}

pub fn colormap_path(pack_root: &FsPath, name: &str) -> FsPath {
    pack_root
        .join("assets")
        .join("minecraft")
        .join("textures")
        .join("colormap")
        .join(format!("{name}.png"))
}

pub fn decode_colormap(bytes: &[u8]) -> Option<Box<[u8]>> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let size = reader.output_buffer_size()?;
    let mut buf = vec![0u8; size];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.bit_depth != png::BitDepth::Eight
        || info.width != COLORMAP_WIDTH
        || info.height != COLORMAP_HEIGHT
    {
        return None;
    }

    let line = info.line_size;
    let row_len = match info.color_type {
        png::ColorType::Rgba => (info.width * 4) as usize,
        png::ColorType::Rgb => (info.width * 3) as usize,
        _ => return None,
    };
    let mut rgb = Vec::with_capacity((info.width * info.height * 3) as usize);
    for y in 0..info.height as usize {
        let row = buf.get(y * line..y * line + row_len)?;
        match info.color_type {
            png::ColorType::Rgba => {
                for px in row.as_chunks::<4>().0 {
                    rgb.extend_from_slice(&px[..3]);
                }
            }
            png::ColorType::Rgb => {
                rgb.extend_from_slice(row);
            }
            _ => return None,
        }
    }
    Some(rgb.into_boxed_slice())
}

pub fn sample_colormap(map: &[u8], temperature: f32, downfall: f32, fallback: [u8; 3]) -> [u8; 3] {
    let temp = temperature.clamp(0.0, 1.0) as f64;
    let rain = downfall.clamp(0.0, 1.0) as f64 * temp;
    let x = ((1.0 - temp) * 255.0) as usize;
    let y = ((1.0 - rain) * 255.0) as usize;
    let base = (y << 8 | x) * 3;
    match map.get(base..base + 3) {
        Some(px) => [px[0], px[1], px[2]],
        None => fallback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tint::rgb;

    #[test]
    fn default_colors_match_vanilla_decimals() {
        assert_eq!(GRASS_DEFAULT, rgb(-65281));
        assert_eq!(FOLIAGE_DEFAULT, rgb(-12012264));
        assert_eq!(DRY_FOLIAGE_DEFAULT, rgb(-10732494));
    }

    #[test]
    fn decode_and_sample_roundtrip() {
        let mut pixels = vec![0u8; 256 * 256 * 3];
        for y in 0..256 {
            for x in 0..256 {
                let base = (y * 256 + x) * 3;
                pixels[base] = x as u8;
                pixels[base + 1] = y as u8;
                pixels[base + 2] = 128;
            }
        }
        let mut encoded = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut encoded, 256, 256);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&pixels)
                .unwrap();
        }
        let map = decode_colormap(&encoded).unwrap();
        assert_eq!(map.len(), 256 * 256 * 3);
        assert_eq!(&map[0..3], &[0, 0, 128]);
        assert_eq!(&map[(255 * 256 + 255) * 3..][..3], &[255, 255, 128]);

        assert_eq!(sample_colormap(&map, 1.0, 1.0, GRASS_DEFAULT), [0, 0, 128]);
        assert_eq!(
            sample_colormap(&map, 0.0, 0.0, GRASS_DEFAULT),
            [255, 255, 128]
        );
        assert_eq!(sample_colormap(&[], 0.5, 0.5, GRASS_DEFAULT), GRASS_DEFAULT);
    }
}
