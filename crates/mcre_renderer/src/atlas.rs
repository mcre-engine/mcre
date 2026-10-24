use std::io::Cursor;

use mcre_fs::{Fs, FsPath};
use thiserror::Error;

use crate::mipmap;

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

/// One entry in an animated sprite's playback timeline.
#[derive(Clone, Debug)]
pub struct AnimationFrame {
    /// Frame index in the source image's grid.
    pub index: usize,
    /// Duration in client ticks.
    pub time: u32,
}

/// RGBA pixels and mip levels for one animation frame.
#[derive(Clone, Debug)]
pub struct AnimationFramePixels {
    pub pixels: Vec<u8>,
    /// Mip levels `1..=mipmap::MIP_LEVELS`.
    pub mips: Vec<Vec<u8>>,
}

/// Animation metadata and decoded frames for one atlas sprite.
#[derive(Clone, Debug)]
pub struct SpriteAnimation {
    pub timeline: Vec<AnimationFrame>,
    pub interpolate: bool,
    pub width: u32,
    pub height: u32,
    /// Indexed by source frame index; `None` for frames not referenced by the
    /// timeline.
    pub frames: Vec<Option<AnimationFramePixels>>,
}

impl SpriteAnimation {
    fn frame_at(&self, ticks: f64) -> (usize, f32) {
        let total: u64 = self
            .timeline
            .iter()
            .map(|frame| u64::from(frame.time))
            .sum();
        if total == 0 {
            return (0, 0.0);
        }
        let mut time = ticks.rem_euclid(total as f64);
        for (index, frame) in self.timeline.iter().enumerate() {
            let duration = f64::from(frame.time);
            if time < duration {
                return (index, (time / duration) as f32);
            }
            time -= duration;
        }
        (self.timeline.len() - 1, 0.0)
    }
}

/// CPU-side RGBA8 texture atlas covering every texture in a pack's texture table.
pub struct Atlas {
    pub width: u32,
    pub height: u32,
    /// Tightly packed RGBA8 rows, `width * 4` bytes each.
    pub pixels: Vec<u8>,
    /// Mip levels `1..=mipmap::MIP_LEVELS` in order; level `n` is
    /// `(width >> n) * (height >> n) * 4` bytes, each sprite downscaled the
    /// way vanilla's `MipmapGenerator` does it.
    pub mips: Vec<Vec<u8>>,
    /// Indexed by `texture_id`; always present for every entry (missing files
    /// become magenta).
    pub regions: Vec<AtlasRegion>,
    /// Parallel to `regions`: whether the region contains any texel with alpha
    /// below 255 (classifies quads into the cutout pass).
    pub has_alpha: Vec<bool>,
    /// Region used when a `texture_id` is out of range.
    pub fallback: AtlasRegion,
    /// Parallel to `regions`: animation data for sprites with a usable
    /// `.png.mcmeta` animation section.
    pub animations: Vec<Option<SpriteAnimation>>,
}

/// Mutable CPU atlas state used to advance animated sprites.
pub struct AtlasAnimator {
    pixels: Vec<u8>,
    mips: Vec<Vec<u8>>,
    /// Current timeline index per animation (parallel to `Atlas::animations`).
    current: Vec<Option<usize>>,
    last_update_ticks: f64,
    last_tick_key: u64,
    any_animated: bool,
    any_interpolated: bool,
}

impl AtlasAnimator {
    pub fn new(atlas: &Atlas) -> Self {
        let any_animated = atlas
            .animations
            .iter()
            .flatten()
            .any(|animation| animation.timeline.len() > 1);
        let any_interpolated = atlas
            .animations
            .iter()
            .flatten()
            .any(|animation| animation.timeline.len() > 1 && animation.interpolate);
        Self {
            pixels: atlas.pixels.clone(),
            mips: atlas.mips.clone(),
            current: atlas
                .animations
                .iter()
                .map(|animation| animation.as_ref().map(|_| 0))
                .collect(),
            last_update_ticks: 0.0,
            last_tick_key: 0,
            any_animated,
            any_interpolated,
        }
    }

    /// Advances the CPU atlas to `ticks` (20 ticks per second). Returns true
    /// when the pixels or mips changed and need re-uploading.
    pub fn update(&mut self, atlas: &Atlas, ticks: f64) -> bool {
        if !self.any_animated {
            return false;
        }
        let tick_key = ticks.floor() as u64;
        let tick_changed = tick_key != self.last_tick_key;
        let interpolation_due = self.any_interpolated && ticks - self.last_update_ticks >= 0.5;
        if !tick_changed && !interpolation_due {
            return false;
        }
        self.last_tick_key = tick_key;
        self.last_update_ticks = ticks;

        let mut changed = false;
        for (index, animation) in atlas.animations.iter().enumerate() {
            let Some(animation) = animation else {
                continue;
            };
            if animation.timeline.len() <= 1 {
                continue;
            }
            let (timeline_index, progress) = animation.frame_at(ticks);
            let timeline_changed = self.current[index] != Some(timeline_index);
            if !timeline_changed && (!animation.interpolate || progress == 0.0) {
                continue;
            }
            self.current[index] = Some(timeline_index);
            let frame = animation.timeline[timeline_index].index;
            let next = animation.timeline[(timeline_index + 1) % animation.timeline.len()].index;
            let Some(frame_pixels) = animation.frames[frame].as_ref() else {
                continue;
            };
            let region = atlas.regions[index];
            if animation.interpolate && next != frame {
                let Some(next_pixels) = animation.frames[next].as_ref() else {
                    continue;
                };
                let pixels = blend(&frame_pixels.pixels, &next_pixels.pixels, progress);
                blit(
                    &mut self.pixels,
                    atlas.width,
                    region.width,
                    region.height,
                    &pixels,
                    region.x,
                    region.y,
                );
                for level in 0..self.mips.len() {
                    let mip = blend(
                        &frame_pixels.mips[level],
                        &next_pixels.mips[level],
                        progress,
                    );
                    let shift = level as u32 + 1;
                    let level_width = (atlas.width >> shift).max(1);
                    let width = (region.width >> shift).max(1);
                    let height = (region.height >> shift).max(1);
                    blit(
                        &mut self.mips[level],
                        level_width,
                        width,
                        height,
                        &mip,
                        region.x >> shift,
                        region.y >> shift,
                    );
                }
            } else {
                blit(
                    &mut self.pixels,
                    atlas.width,
                    region.width,
                    region.height,
                    &frame_pixels.pixels,
                    region.x,
                    region.y,
                );
                for (level, mip) in frame_pixels.mips.iter().enumerate() {
                    let shift = level as u32 + 1;
                    let level_width = (atlas.width >> shift).max(1);
                    let width = (region.width >> shift).max(1);
                    let height = (region.height >> shift).max(1);
                    blit(
                        &mut self.mips[level],
                        level_width,
                        width,
                        height,
                        mip,
                        region.x >> shift,
                        region.y >> shift,
                    );
                }
            }
            changed = true;
        }
        changed
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn mips(&self) -> &[Vec<u8>] {
        &self.mips
    }
}

fn blend(from: &[u8], to: &[u8], progress: f32) -> Vec<u8> {
    from.iter()
        .zip(to)
        .map(|(&from, &to)| {
            (f32::from(from) + (f32::from(to) - f32::from(from)) * progress).round() as u8
        })
        .collect()
}

impl Atlas {
    pub fn region(&self, texture_id: u32) -> AtlasRegion {
        self.regions
            .get(texture_id as usize)
            .copied()
            .unwrap_or(self.fallback)
    }

    pub fn region_has_alpha(&self, texture_id: u32) -> bool {
        self.has_alpha
            .get(texture_id as usize)
            .copied()
            .unwrap_or(false)
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
/// A sibling `.png.mcmeta` animation section selects the first timeline frame
/// and keeps the remaining frames for [`AtlasAnimator`]. Files that are
/// missing or fail to decode fall back to a magenta placeholder.
pub async fn build_atlas<F: Fs>(
    fs: &F,
    pack_roots: &[FsPath],
    texture_table: &[Box<str>],
) -> Result<Atlas, AtlasError> {
    build_atlas_impl(fs, pack_roots, texture_table, false).await
}

/// Like [`build_atlas`], but pads the packed layout to a square.
///
/// Vanilla's atlas can be non-square, but the RGSS shader's mip selection
/// assumes square texels. Small packs (or packs with very few textures)
/// otherwise pack into a wide, short atlas and sample several mip levels too
/// high. The browser viewer (and the native demo reference it is compared
/// against) uses this for user-provided packs.
pub async fn build_atlas_square<F: Fs>(
    fs: &F,
    pack_roots: &[FsPath],
    texture_table: &[Box<str>],
) -> Result<Atlas, AtlasError> {
    build_atlas_impl(fs, pack_roots, texture_table, true).await
}

async fn build_atlas_impl<F: Fs>(
    fs: &F,
    pack_roots: &[FsPath],
    texture_table: &[Box<str>],
    square: bool,
) -> Result<Atlas, AtlasError> {
    let mut sources = Vec::with_capacity(texture_table.len());
    for texture in texture_table {
        let mut source = None;
        for pack_root in pack_roots.iter().rev() {
            let path = texture_path(pack_root, texture);
            match fs.read(&path).await {
                Ok(bytes) => {
                    let image = decode_png(&bytes);
                    let animation = match image.as_ref() {
                        Some(image) => read_animation(fs, pack_root, texture, image).await,
                        None => None,
                    };
                    source = Some((image, animation));
                    break;
                }
                Err(_) => continue,
            }
        }
        sources.push(source);
    }

    let mut images = Vec::with_capacity(sources.len() + 1);
    let mut animations = Vec::with_capacity(sources.len() + 1);
    for source in sources {
        let (image, animation) = source.unwrap_or((None, None));
        let image = image.unwrap_or_else(fallback_image);
        match animation {
            Some(Some(animation)) => {
                let first = animation.frames[animation.timeline[0].index]
                    .as_ref()
                    .expect("timeline frames are decoded");
                images.push(Image {
                    width: animation.width,
                    height: animation.height,
                    rgba: first.pixels.clone(),
                });
                animations.push(Some(animation));
            }
            Some(None) => {
                images.push(crop_first_frame(image));
                animations.push(None);
            }
            None => {
                images.push(image);
                animations.push(None);
            }
        }
    }

    let fallback_index = images.len();
    images.push(fallback_image());
    animations.push(None);

    let (width, height, positions) = pack(&images, MAX_ATLAS_SIZE, square)?;

    // Vanilla classifies each sprite as `MEAN` or `CUTOUT` and, for cutout
    // sprites, bleeds opaque colors under the transparent texels before the
    // mip chain is generated (`MipmapGenerator` + `TextureUtil.solidify`).
    let mut cutout = vec![false; images.len()];
    let mut coverage = vec![0.0_f32; images.len()];
    for (index, image) in images.iter_mut().enumerate() {
        cutout[index] = mipmap::has_transparent(&image.rgba);
        if cutout[index] {
            mipmap::solidify(image.width, image.height, &mut image.rgba);
            coverage[index] = mipmap::alpha_test_coverage(
                image.width,
                image.height,
                &image.rgba,
                mipmap::ALPHA_CUTOFF,
                1.0,
            );
        }
    }

    let mut pixels = vec![0u8; (width * height * 4) as usize];
    for (image, &(x, y)) in images.iter().zip(&positions) {
        blit(
            &mut pixels,
            width,
            image.width,
            image.height,
            &image.rgba,
            x,
            y,
        );
    }

    let mut mips = Vec::with_capacity(mipmap::MIP_LEVELS as usize);
    let mut levels: Vec<(u32, u32, Vec<u8>)> = images
        .iter()
        .map(|image| (image.width, image.height, image.rgba.clone()))
        .collect();
    for level in 1..=mipmap::MIP_LEVELS {
        let (level_width, level_height) = ((width >> level).max(1), (height >> level).max(1));
        let mut level_pixels = vec![0u8; (level_width * level_height * 4) as usize];
        for (sprite, image) in levels.iter_mut().enumerate() {
            let (next_width, next_height, next) = mipmap::next_level(
                image.0,
                image.1,
                &image.2,
                cutout[sprite],
                coverage[sprite],
                0.0,
            );
            let (x, y) = positions[sprite];
            blit(
                &mut level_pixels,
                level_width,
                next_width,
                next_height,
                &next,
                x >> level,
                y >> level,
            );
            *image = (next_width, next_height, next);
        }
        mips.push(level_pixels);
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

    let has_alpha: Vec<bool> = images[..fallback_index]
        .iter()
        .enumerate()
        .map(|(index, image)| {
            animations[index].as_ref().map_or_else(
                || image.rgba.as_chunks::<4>().0.iter().any(|px| px[3] != 255),
                |animation| {
                    animation.frames.iter().flatten().any(|frame| {
                        frame
                            .pixels
                            .as_chunks::<4>()
                            .0
                            .iter()
                            .any(|px| px[3] != 255)
                    })
                },
            )
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
        mips,
        regions,
        has_alpha,
        fallback,
        animations,
    })
}

#[derive(serde::Deserialize)]
struct McMeta {
    animation: Option<AnimationMeta>,
}

#[derive(serde::Deserialize)]
struct AnimationMeta {
    #[serde(default = "default_frametime")]
    frametime: u32,
    frames: Option<Vec<FrameSpec>>,
    #[serde(default)]
    interpolate: bool,
    width: Option<u32>,
    height: Option<u32>,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum FrameSpec {
    Index(i32),
    Timed { index: i32, time: Option<i32> },
}

fn default_frametime() -> u32 {
    1
}

async fn read_animation<F: Fs>(
    fs: &F,
    pack_root: &FsPath,
    texture: &str,
    image: &Image,
) -> Option<Option<SpriteAnimation>> {
    let path = texture_path(pack_root, texture);
    let meta_path = FsPath::from(format!("{path}.mcmeta").as_str());
    let bytes = fs.read(&meta_path).await.ok()?;
    let meta: McMeta = serde_json::from_slice(&bytes).ok()?;
    Some(parse_animation(image, &meta.animation?))
}

fn parse_animation(image: &Image, meta: &AnimationMeta) -> Option<SpriteAnimation> {
    if meta.frametime == 0 {
        return None;
    }
    let (frame_width, frame_height) = match (meta.width, meta.height) {
        (Some(width), Some(height)) => (width, height),
        (Some(width), None) => (width, image.height),
        (None, Some(height)) => (image.width, height),
        (None, None) => {
            let side = image.width.min(image.height);
            (side, side)
        }
    };
    if frame_width == 0
        || frame_height == 0
        || frame_width > image.width
        || frame_height > image.height
    {
        return None;
    }
    let columns = image.width / frame_width;
    let rows = image.height / frame_height;
    let total = columns.checked_mul(rows)?;
    if total == 0 {
        return None;
    }

    let timeline: Vec<AnimationFrame> = match &meta.frames {
        Some(frames) => frames
            .iter()
            .filter_map(|frame| {
                let (index, time) = match frame {
                    FrameSpec::Index(index) => (*index, meta.frametime as i32),
                    FrameSpec::Timed { index, time } => {
                        (*index, time.unwrap_or(meta.frametime as i32))
                    }
                };
                if index < 0 || index as u32 >= total || time <= 0 {
                    return None;
                }
                Some(AnimationFrame {
                    index: index as usize,
                    time: time as u32,
                })
            })
            .collect(),
        None => (0..total as usize)
            .map(|index| AnimationFrame {
                index,
                time: meta.frametime,
            })
            .collect(),
    };
    if timeline.is_empty() {
        return None;
    }

    let mut frames: Vec<Option<AnimationFramePixels>> = (0..total).map(|_| None).collect();
    for frame in &timeline {
        if frames[frame.index].is_none() {
            frames[frame.index] = Some(animation_frame(
                image,
                frame_width,
                frame_height,
                columns,
                frame.index,
            ));
        }
    }

    Some(SpriteAnimation {
        timeline,
        interpolate: meta.interpolate,
        width: frame_width,
        height: frame_height,
        frames,
    })
}

fn animation_frame(
    image: &Image,
    width: u32,
    height: u32,
    columns: u32,
    index: usize,
) -> AnimationFramePixels {
    let x = (index as u32 % columns) * width;
    let y = (index as u32 / columns) * height;
    let row_bytes = (width * 4) as usize;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        let start = (((y + row) * image.width + x) * 4) as usize;
        rgba.extend_from_slice(&image.rgba[start..start + row_bytes]);
    }
    let cutout = mipmap::has_transparent(&rgba);
    let coverage = if cutout {
        mipmap::solidify(width, height, &mut rgba);
        mipmap::alpha_test_coverage(width, height, &rgba, mipmap::ALPHA_CUTOFF, 1.0)
    } else {
        0.0
    };
    let mips = mip_chain(width, height, &rgba, cutout, coverage);
    AnimationFramePixels { pixels: rgba, mips }
}

fn mip_chain(
    mut width: u32,
    mut height: u32,
    rgba: &[u8],
    cutout: bool,
    coverage: f32,
) -> Vec<Vec<u8>> {
    let mut levels = Vec::with_capacity(mipmap::MIP_LEVELS as usize);
    let mut current = rgba.to_vec();
    for _ in 0..mipmap::MIP_LEVELS {
        let (next_width, next_height, next) =
            mipmap::next_level(width, height, &current, cutout, coverage, 0.0);
        levels.push(next.clone());
        width = next_width;
        height = next_height;
        current = next;
    }
    levels
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

pub(crate) fn decode_png_rgba(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let image = decode_png(bytes)?;
    Some((image.width, image.height, image.rgba))
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
    Some(image)
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

fn pack(images: &[Image], max_size: u32, square: bool) -> Result<PackedLayout, AtlasError> {
    let area: u64 = images
        .iter()
        .map(|i| u64::from(i.width) * u64::from(i.height))
        .sum();
    let mut side = (area as f64).sqrt().ceil() as u32;
    side = side.next_power_of_two().clamp(256, max_size);

    loop {
        if let Some((width, height, positions)) = shelf_pack(images, side, square) {
            return Ok((width, height, positions));
        }
        if side >= max_size {
            return Err(AtlasError::TooLarge(max_size));
        }
        side = (side * 2).min(max_size);
    }
}

fn shelf_pack(images: &[Image], max_width: u32, square: bool) -> Option<PackedLayout> {
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
    let used_height = y.checked_add(row_height)?;
    if used_height > max_width {
        return None;
    }
    let height = if square {
        max_width
    } else {
        used_height.next_power_of_two().max(1)
    };
    Some((width, height, positions))
}

fn blit(atlas: &mut [u8], atlas_width: u32, width: u32, height: u32, rgba: &[u8], x: u32, y: u32) {
    let row_bytes = (width * 4) as usize;
    for row in 0..height as usize {
        let src = &rgba[row * row_bytes..(row + 1) * row_bytes];
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
        let (width, height, positions) = pack(&images, MAX_ATLAS_SIZE, false).expect("pack");
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
    fn square_pack_is_square() {
        let images: Vec<Image> = vec![image(16, 16), image(16, 16), image(16, 16)];
        let (width, height, positions) = pack(&images, MAX_ATLAS_SIZE, true).expect("pack");
        assert_eq!(width, height);
        assert_eq!(positions.len(), images.len());
        for (img, (x, y)) in images.iter().zip(positions) {
            assert!(x + img.width <= width);
            assert!(y + img.height <= height);
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

    fn translucent_png() -> Vec<u8> {
        let mut pixels = Vec::with_capacity(16 * 16 * 4);
        for i in 0..16 * 16 {
            let a = if i == 0 { 128 } else { 255 };
            pixels.extend_from_slice(&[10, 20, 30, a]);
        }
        let mut encoded = Vec::new();
        let mut encoder = png::Encoder::new(&mut encoded, 16, 16);
        encoder.set_color(png::ColorType::Rgba);
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
        assert!(!atlas.region_has_alpha(0));
        assert!(!atlas.region_has_alpha(99));

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn region_has_alpha_scans_texels() {
        use mcre_fs::native::NativeFs;

        let root =
            std::env::temp_dir().join(format!("mcre-atlas-alpha-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let fs = NativeFs::new_custom_path(root.clone()).await.unwrap();

        let dir = root.join("base").join("assets/minecraft/textures/block");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("opaque.png"), solid_png([1, 2, 3])).unwrap();
        std::fs::write(dir.join("glass.png"), translucent_png()).unwrap();

        let table: Vec<Box<str>> = vec![
            "minecraft:block/opaque".into(),
            "minecraft:block/glass".into(),
        ];
        let base = FsPath::from("base");
        let atlas = build_atlas(&fs, &[base], &table).await.unwrap();
        assert!(!atlas.region_has_alpha(0));
        assert!(atlas.region_has_alpha(1));

        std::fs::remove_dir_all(&root).unwrap();
    }

    fn animation_image(width: u32, height: u32, columns: u32) -> Image {
        let mut image = image(width, height);
        let frame_width = width / columns;
        let frame_height = frame_width;
        let rows = height / frame_height;
        for frame in 0..columns * rows {
            let x0 = (frame % columns) * frame_width;
            let y0 = (frame / columns) * frame_height;
            for y in y0..y0 + frame_height {
                for x in x0..x0 + frame_width {
                    let base = ((y * width + x) * 4) as usize;
                    image.rgba[base] = frame as u8;
                    image.rgba[base + 3] = 255;
                }
            }
        }
        image
    }

    fn animated_png() -> Vec<u8> {
        let mut pixels = Vec::with_capacity(16 * 64 * 3);
        for frame in 0..4 {
            for _ in 0..16 * 16 {
                pixels.extend_from_slice(&[frame as u8, 0, 0]);
            }
        }
        let mut encoded = Vec::new();
        let mut encoder = png::Encoder::new(&mut encoded, 16, 64);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&pixels)
            .unwrap();
        encoded
    }

    #[tokio::test]
    async fn build_atlas_reads_animation_metadata() {
        use mcre_fs::native::NativeFs;

        let root =
            std::env::temp_dir().join(format!("mcre-atlas-animation-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let fs = NativeFs::new_custom_path(root.clone()).await.unwrap();

        let dir = root.join("base").join("assets/minecraft/textures/block");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("animated.png"), animated_png()).unwrap();
        std::fs::write(
            dir.join("animated.png.mcmeta"),
            r#"{"animation":{"frametime":1}}"#,
        )
        .unwrap();

        let table: Vec<Box<str>> = vec!["minecraft:block/animated".into()];
        let atlas = build_atlas(&fs, &[FsPath::from("base")], &table)
            .await
            .unwrap();
        let region = atlas.region(0);
        assert_eq!((region.width, region.height), (16, 16));
        assert!(atlas.animations[0].is_some());
        assert_eq!(atlas_center(&atlas, 0), [0, 0, 0]);

        let mut animator = AtlasAnimator::new(&atlas);
        assert!(!animator.update(&atlas, 0.0));
        assert!(animator.update(&atlas, 1.0));
        let x = region.x as usize + region.width as usize / 2;
        let y = region.y as usize + region.height as usize / 2;
        let base = (y * atlas.width as usize + x) * 4;
        assert_eq!(animator.pixels()[base], 1);

        std::fs::remove_dir_all(&root).unwrap();
    }

    fn parse_test_animation(json: &str, image: &Image) -> Option<SpriteAnimation> {
        let meta: McMeta = serde_json::from_str(json).unwrap();
        parse_animation(image, &meta.animation.unwrap())
    }

    fn animation_atlas(animation: SpriteAnimation) -> Atlas {
        let first = animation.frames[animation.timeline[0].index]
            .as_ref()
            .expect("first frame");
        let width = animation.width;
        let height = animation.height;
        Atlas {
            width,
            height,
            pixels: first.pixels.clone(),
            mips: first.mips.clone(),
            regions: vec![AtlasRegion {
                x: 0,
                y: 0,
                width: animation.width,
                height: animation.height,
            }],
            has_alpha: vec![false],
            fallback: AtlasRegion {
                x: 0,
                y: 0,
                width,
                height,
            },
            animations: vec![Some(animation)],
        }
    }

    #[test]
    fn parses_vertical_animation_strip() {
        let image = animation_image(16, 64, 1);
        let animation =
            parse_test_animation(r#"{"animation":{"frametime":2}}"#, &image).expect("animation");
        assert_eq!(animation.width, 16);
        assert_eq!(animation.height, 16);
        assert_eq!(animation.timeline.len(), 4);
        assert_eq!(animation.timeline[2].time, 2);
        assert_eq!(
            animation.frames[2].as_ref().unwrap().pixels[0],
            2,
            "frame 2 comes from the third strip cell"
        );
        assert_eq!(animation.frame_at(0.0), (0, 0.0));
        let (timeline, progress) = animation.frame_at(3.0);
        assert_eq!(timeline, 1);
        assert!((progress - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn parses_grid_animation_and_explicit_times() {
        let image = animation_image(32, 16, 2);
        let animation = parse_test_animation(
            r#"{"animation":{"width":16,"height":16,"frametime":1,"interpolate":true,"frames":[0,{"index":1,"time":3}]}}"#,
            &image,
        )
        .expect("animation");
        assert_eq!(animation.timeline.len(), 2);
        assert_eq!(animation.timeline[1].index, 1);
        assert_eq!(animation.timeline[1].time, 3);
        assert!(animation.interpolate);
        assert_eq!(animation.frames[1].as_ref().unwrap().pixels[0], 1);
    }

    #[test]
    fn animation_skips_invalid_frames() {
        let image = animation_image(16, 64, 1);
        let animation = parse_test_animation(
            r#"{"animation":{"frames":[0,99,{"index":1,"time":0},2]}}"#,
            &image,
        )
        .expect("animation");
        assert_eq!(
            animation
                .timeline
                .iter()
                .map(|frame| frame.index)
                .collect::<Vec<_>>(),
            vec![0, 2]
        );
    }

    #[test]
    fn single_frame_animation_keeps_frame_size() {
        let image = animation_image(16, 32, 1);
        let animation =
            parse_test_animation(r#"{"animation":{"frames":[0]}}"#, &image).expect("animation");
        assert_eq!(animation.timeline.len(), 1);
        assert_eq!((animation.width, animation.height), (16, 16));
    }

    #[test]
    fn animator_blits_timeline_frames() {
        let image = animation_image(16, 64, 1);
        let animation =
            parse_test_animation(r#"{"animation":{"frametime":2}}"#, &image).expect("animation");
        let atlas = animation_atlas(animation);
        let mut animator = AtlasAnimator::new(&atlas);
        assert!(!animator.update(&atlas, 0.0));
        assert!(animator.update(&atlas, 2.0));
        assert_eq!(animator.pixels()[0], 1);
        assert!(animator.update(&atlas, 4.0));
        assert_eq!(animator.pixels()[0], 2);
    }

    #[test]
    fn animator_interpolates_frames() {
        let image = animation_image(16, 32, 1);
        let animation = parse_test_animation(
            r#"{"animation":{"frametime":2,"interpolate":true}}"#,
            &image,
        )
        .expect("animation");
        let atlas = animation_atlas(animation);
        let mut animator = AtlasAnimator::new(&atlas);
        assert!(animator.update(&atlas, 1.0));
        assert_eq!(animator.pixels()[0], 1);
    }
}
