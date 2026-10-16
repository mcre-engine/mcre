use std::io::Cursor;

use mcre_fs::{Fs, FsPath};
use thiserror::Error;

const FALLBACK_SIZE: u32 = 16;
const MAX_ATLAS_SIZE: u32 = 4096;

/// A sub-rectangle of the atlas texture (pixel coordinates, top-left origin).
#[derive(Clone, Copy, Debug)]
pub struct AtlasRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// CPU-side RGBA8 texture atlas covering every texture in a pack's texture table.
pub struct Atlas {
    pub width: u32,
    pub height: u32,
    /// Tightly packed RGBA8 rows, `width * 4` bytes each.
    pub pixels: Vec<u8>,
    /// Indexed by `texture_id`; always present for every entry (missing files
    /// become magenta).
    pub regions: Vec<AtlasRegion>,
    /// Region used when a `texture_id` is out of range.
    pub fallback: AtlasRegion,
}

impl Atlas {
    pub fn region(&self, texture_id: u32) -> AtlasRegion {
        self.regions
            .get(texture_id as usize)
            .copied()
            .unwrap_or(self.fallback)
    }
}

#[derive(Debug, Error)]
pub enum AtlasError {
    #[error("texture atlas exceeds the maximum size of {0}x{0}")]
    TooLarge(u32),
}

struct Image {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

/// Builds the atlas from every texture path in `texture_table`.
///
/// Files are looked up at `{pack_root}/assets/{namespace}/textures/{path}.png`.
/// Animated textures are cropped to their first square frame. Files that are
/// missing or fail to decode fall back to a magenta placeholder.
pub async fn build_atlas<F: Fs>(
    fs: &F,
    pack_roots: &[FsPath],
    texture_table: &[Box<str>],
) -> Result<Atlas, AtlasError> {
    let mut images = Vec::with_capacity(texture_table.len());
    for texture in texture_table {
        let mut image = None;
        for pack_root in pack_roots.iter().rev() {
            let path = texture_path(pack_root, texture);
            match fs.read(&path).await {
                Ok(bytes) => {
                    image = Some(decode_png(&bytes).unwrap_or_else(fallback_image));
                    break;
                }
                Err(_) => continue,
            }
        }
        images.push(image.unwrap_or_else(fallback_image));
    }

    let fallback_index = images.len();
    images.push(fallback_image());

    let (width, height, positions) = pack(&images, MAX_ATLAS_SIZE)?;
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    for (image, &(x, y)) in images.iter().zip(&positions) {
        blit(&mut pixels, width, image, x, y);
    }

    let regions: Vec<AtlasRegion> = positions[..fallback_index]
        .iter()
        .zip(&images[..fallback_index])
        .map(|(&(x, y), image)| AtlasRegion {
            x,
            y,
            width: image.width,
            height: image.height,
        })
        .collect();

    let fallback = AtlasRegion {
        x: positions[fallback_index].0,
        y: positions[fallback_index].1,
        width: images[fallback_index].width,
        height: images[fallback_index].height,
    };

    Ok(Atlas {
        width,
        height,
        pixels,
        regions,
        fallback,
    })
}

fn texture_path(pack_root: &FsPath, texture: &str) -> FsPath {
    let (namespace, path) = texture.split_once(':').unwrap_or(("minecraft", texture));
    FsPath::from(format!("{pack_root}/assets/{namespace}/textures/{path}.png").as_str())
}

fn fallback_image() -> Image {
    let mut rgba = Vec::with_capacity((FALLBACK_SIZE * FALLBACK_SIZE * 4) as usize);
    for _ in 0..FALLBACK_SIZE * FALLBACK_SIZE {
        rgba.extend_from_slice(&[255, 0, 255, 255]);
    }
    Image {
        width: FALLBACK_SIZE,
        height: FALLBACK_SIZE,
        rgba,
    }
}

fn decode_png(bytes: &[u8]) -> Option<Image> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let size = reader.output_buffer_size()?;
    let mut buf = vec![0u8; size];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.bit_depth != png::BitDepth::Eight {
        return None;
    }

    let width = info.width;
    let height = info.height;
    let line = info.line_size;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    match info.color_type {
        png::ColorType::Rgba => {
            let row_len = (width * 4) as usize;
            for y in 0..height as usize {
                let row = buf.get(y * line..y * line + row_len)?;
                rgba.extend_from_slice(row);
            }
        }
        png::ColorType::Rgb => {
            for y in 0..height as usize {
                let row = buf.get(y * line..y * line + width as usize * 3)?;
                for px in row.as_chunks::<3>().0 {
                    rgba.extend_from_slice(px);
                    rgba.push(255);
                }
            }
        }
        png::ColorType::Grayscale => {
            for y in 0..height as usize {
                let row = buf.get(y * line..y * line + width as usize)?;
                for &g in row {
                    rgba.extend_from_slice(&[g, g, g, 255]);
                }
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for y in 0..height as usize {
                let row = buf.get(y * line..y * line + width as usize * 2)?;
                for px in row.as_chunks::<2>().0 {
                    let (g, a) = (px[0], px[1]);
                    rgba.extend_from_slice(&[g, g, g, a]);
                }
            }
        }
        _ => return None,
    }

    let image = Image {
        width,
        height,
        rgba,
    };
    Some(crop_first_frame(image))
}

/// Crops an image (animated strips are `width x n*width`) to its top square frame.
fn crop_first_frame(image: Image) -> Image {
    let side = image.width.min(image.height);
    if image.width == side && image.height == side {
        return image;
    }
    let mut rgba = Vec::with_capacity((side * side * 4) as usize);
    let src_stride = (image.width * 4) as usize;
    for y in 0..side as usize {
        let start = y * src_stride;
        rgba.extend_from_slice(&image.rgba[start..start + side as usize * 4]);
    }
    Image {
        width: side,
        height: side,
        rgba,
    }
}

/// Shelf-packs images into the smallest power-of-two square that fits.
/// Returns `(width, height, positions)`.
type PackedLayout = (u32, u32, Vec<(u32, u32)>);

fn pack(images: &[Image], max_size: u32) -> Result<PackedLayout, AtlasError> {
    let area: u64 = images
        .iter()
        .map(|i| u64::from(i.width) * u64::from(i.height))
        .sum();
    let mut side = (area as f64).sqrt().ceil() as u32;
    side = side.next_power_of_two().clamp(256, max_size);

    loop {
        if let Some((width, height, positions)) = shelf_pack(images, side) {
            return Ok((width, height, positions));
        }
        if side >= max_size {
            return Err(AtlasError::TooLarge(max_size));
        }
        side = (side * 2).min(max_size);
    }
}

fn shelf_pack(images: &[Image], max_width: u32) -> Option<PackedLayout> {
    let mut order: Vec<usize> = (0..images.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(images[i].height));

    let mut positions = vec![(0u32, 0u32); images.len()];
    let (mut x, mut y, mut row_height) = (0u32, 0u32, 0u32);
    for index in order {
        let image = &images[index];
        if image.width > max_width {
            return None;
        }
        if x + image.width > max_width {
            y = y.checked_add(row_height)?;
            x = 0;
            row_height = 0;
        }
        positions[index] = (x, y);
        x += image.width;
        row_height = row_height.max(image.height);
    }

    let width = max_width;
    let height = y.checked_add(row_height)?.next_power_of_two().max(1);
    if height > max_width {
        return None;
    }
    Some((width, height, positions))
}

fn blit(atlas: &mut [u8], atlas_width: u32, image: &Image, x: u32, y: u32) {
    let row_bytes = (image.width * 4) as usize;
    for row in 0..image.height as usize {
        let src = &image.rgba[row * row_bytes..(row + 1) * row_bytes];
        let dst_start = (((y as usize + row) * atlas_width as usize) + x as usize) * 4;
        atlas[dst_start..dst_start + row_bytes].copy_from_slice(src);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: u32, height: u32) -> Image {
        Image {
            width,
            height,
            rgba: vec![0; (width * height * 4) as usize],
        }
    }

    #[test]
    fn shelf_pack_fits_all_images() {
        let images: Vec<Image> = vec![
            image(16, 16),
            image(16, 64),
            image(32, 32),
            image(8, 8),
            image(16, 4),
        ];
        let (width, height, positions) = pack(&images, MAX_ATLAS_SIZE).expect("pack");
        assert!(width <= MAX_ATLAS_SIZE && height <= MAX_ATLAS_SIZE);
        assert_eq!(positions.len(), images.len());

        for (i, img) in images.iter().enumerate() {
            let (x, y) = positions[i];
            assert!(x + img.width <= width, "image {i} overflows width");
            assert!(y + img.height <= height, "image {i} overflows height");
            for (j, other) in images.iter().enumerate() {
                if i == j {
                    continue;
                }
                let (ox, oy) = positions[j];
                let separate = x + img.width <= ox
                    || ox + other.width <= x
                    || y + img.height <= oy
                    || oy + other.height <= y;
                assert!(separate, "images {i} and {j} overlap");
            }
        }
    }

    #[test]
    fn texture_path_convention() {
        let root = FsPath::from("minecraft/resource_packs/1.0/default");
        let path = texture_path(&root, "minecraft:block/stone");
        assert_eq!(
            path.to_opfs_string(),
            "minecraft/resource_packs/1.0/default/assets/minecraft/textures/block/stone.png"
        );
    }

    #[test]
    fn crop_takes_top_square_frame() {
        let img = image(16, 64);
        let cropped = crop_first_frame(img);
        assert_eq!(cropped.width, 16);
        assert_eq!(cropped.height, 16);
        assert_eq!(cropped.rgba.len(), 16 * 16 * 4);
    }

    fn solid_png(rgb: [u8; 3]) -> Vec<u8> {
        let mut pixels = Vec::with_capacity(16 * 16 * 3);
        for _ in 0..16 * 16 {
            pixels.extend_from_slice(&rgb);
        }
        let mut encoded = Vec::new();
        let mut encoder = png::Encoder::new(&mut encoded, 16, 16);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&pixels)
            .unwrap();
        encoded
    }

    fn atlas_center(atlas: &Atlas, texture_id: u32) -> [u8; 3] {
        let region = atlas.region(texture_id);
        let x = region.x as usize + region.width as usize / 2;
        let y = region.y as usize + region.height as usize / 2;
        let base = (y * atlas.width as usize + x) * 4;
        atlas.pixels[base..base + 3].try_into().unwrap()
    }

    #[tokio::test]
    async fn atlas_resolves_textures_top_first() {
        use mcre_fs::native::NativeFs;

        let root = std::env::temp_dir().join(format!("mcre-atlas-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let fs = NativeFs::new_custom_path(root.clone()).await.unwrap();
        let write = |pack: &str, rgb: [u8; 3]| {
            let dir = root.join(pack).join("assets/minecraft/textures/block");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("stone.png"), solid_png(rgb)).unwrap();
        };
        write("base", [255, 0, 0]);
        write("top", [0, 255, 0]);

        let table: Vec<Box<str>> = vec!["minecraft:block/stone".into()];
        let base = FsPath::from("base");
        let top = FsPath::from("top");

        let atlas = build_atlas(&fs, &[base.clone(), top.clone()], &table)
            .await
            .unwrap();
        assert_eq!(atlas_center(&atlas, 0), [0, 255, 0]);

        let atlas = build_atlas(&fs, &[top, base.clone()], &table)
            .await
            .unwrap();
        assert_eq!(atlas_center(&atlas, 0), [255, 0, 0]);

        let missing: Vec<Box<str>> = vec!["minecraft:block/dirt".into()];
        let atlas = build_atlas(&fs, &[base], &missing).await.unwrap();
        assert_eq!(atlas_center(&atlas, 0), [255, 0, 255]);

        std::fs::remove_dir_all(&root).unwrap();
    }
}
