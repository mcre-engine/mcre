//! CPU port of vanilla `CloudRenderer` (26.4).
//!
//! The cloud layer is a 12x12-block cell grid sampled from
//! `textures/environment/clouds.png`; cells with alpha >= 10 become quads.
//! `CloudStatus::Fancy` extrudes each cell into a 4-block-tall box (only the
//! boundary faces looking at empty neighbors), `CloudStatus::Fast` emits one
//! top-colored quad per cell. The mesh is rebuilt only when the camera
//! crosses a cell boundary or the status/height changes, mirroring vanilla's
//! cell-anchored scrolling.

use mcre_fs::{Fs, FsPath};

/// Side length of one cloud cell in blocks (`CELL_SIZE_IN_BLOCKS`).
pub const CELL_SIZE: f32 = 12.0;
/// Extruded cloud height in blocks.
pub const CELL_HEIGHT: f32 = 4.0;
/// Ticks it takes for the cloud pattern to advance by one cell.
pub const TICKS_PER_CELL: i64 = 400;
/// Vanilla `EnvironmentAttributes.CLOUD_HEIGHT` default.
pub const CLOUD_HEIGHT: f32 = 192.33;
/// Vanilla `Options.cloudRange` default, in chunks.
pub const DEFAULT_CLOUD_RANGE: u32 = 128;
/// Vanilla `EnvironmentAttributes.CLOUD_FOG_END_DISTANCE` default.
pub const CLOUD_FOG_END_DISTANCE: f32 = 2048.0;

const FLAG_INSIDE_FACE: u32 = 16;
const FLAG_USE_TOP_COLOR: u32 = 32;
const FLAG_MASK_DIR: u32 = 7;

const COLOR_OFFSET: u32 = 4;
const NORTH_OFFSET: u32 = 3;
const EAST_OFFSET: u32 = 2;
const SOUTH_OFFSET: u32 = 1;
const WEST_OFFSET: u32 = 0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CloudStatus {
    Off,
    Fast,
    #[default]
    Fancy,
}

/// Where the camera sits relative to the 4-block cloud slab; drives which
/// horizontal cap faces are emitted and whether the inside hull is visible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelativeCameraPos {
    AboveClouds,
    InsideClouds,
    BelowClouds,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CloudVertex {
    /// Cell coordinates relative to the mesh center cell.
    pub cell: [f32; 2],
    /// `direction.get3DDataValue() | inside-face | use-top-color` flags.
    pub dir_flags: u32,
    /// Quad corner index (`i % 4` of the QUADS index pattern); the shader
    /// reverses this for inside faces.
    pub corner: u32,
}

/// Vertex index pattern of `sharedSequentialQuad`: `0, 1, 2, 2, 3, 0`.
const QUAD_CORNERS: [u32; 6] = [0, 1, 2, 2, 3, 0];

/// One row-major cloud cell: `color << 4 | north | east << 1... ` packed
/// exactly like vanilla `packCellData`. Only alpha and the four
/// `is*Empty` neighbor bits are ever read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloudCell(u64);

impl CloudCell {
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    fn neighbor_empty(self, offset: u32) -> bool {
        !self.is_empty() && (self.0 >> offset & 1) != 0
    }
}

#[derive(Clone, Debug)]
pub struct CloudTexture {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<CloudCell>,
}

impl CloudTexture {
    /// Builds the cell table from top-down RGBA8 pixels, packing the four
    /// neighbor-emptiness bits with wraparound. Vanilla wraps the east/west
    /// lookups by `height` (only correct for square textures); this uses
    /// `width`, which is identical for the 256x256 vanilla texture.
    pub fn from_rgba(width: usize, height: usize, rgba: &[u8]) -> Option<Self> {
        if width == 0 || height == 0 || rgba.len() < width * height * 4 {
            return None;
        }
        let alpha_at = |x: usize, y: usize| rgba[(y * width + x) * 4 + 3];
        let empty = |x: usize, y: usize| alpha_at(x, y) < 10;

        let mut cells = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                if empty(x, y) {
                    cells.push(CloudCell(0));
                    continue;
                }
                let north = empty(x, (y + height - 1) % height);
                let east = empty((x + 1) % width, y);
                let south = empty(x, (y + 1) % height);
                let west = empty((x + width - 1) % width, y);
                let color = u64::from(alpha_at(x, y)) << 24; // alpha in the color field
                let packed = color << COLOR_OFFSET
                    | u64::from(north) << NORTH_OFFSET
                    | u64::from(east) << EAST_OFFSET
                    | u64::from(south) << SOUTH_OFFSET
                    | u64::from(west);
                cells.push(CloudCell(packed));
            }
        }
        Some(Self {
            width,
            height,
            cells,
        })
    }

    /// Loads `textures/environment/clouds.png` from the pack stack
    /// (top-first, like the atlas builder). Missing or broken textures
    /// return `None` and clouds are simply not drawn.
    pub async fn load<F: Fs>(fs: &F, pack_roots: &[FsPath]) -> Option<Self> {
        for pack_root in pack_roots.iter().rev() {
            let path = FsPath::from(
                format!("{pack_root}/assets/minecraft/textures/environment/clouds.png").as_str(),
            );
            let Ok(bytes) = fs.read(&path).await else {
                continue;
            };
            let (width, height, rgba) = crate::atlas::decode_png_rgba(&bytes)?;
            return Self::from_rgba(width as usize, height as usize, &rgba);
        }
        None
    }

    fn cell_at(&self, x: i32, z: i32) -> CloudCell {
        let index = x.rem_euclid(self.width as i32) as usize
            + z.rem_euclid(self.height as i32) as usize * self.width;
        self.cells[index]
    }
}

/// Scrolling state derived from the camera and game time (vanilla
/// `CloudRenderer.prepare`). `offset` positions the mesh in world space for
/// our absolute-coordinate view matrix: vanilla renders camera-relative, but
/// adding the camera back is identical modulo the texture wrap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CloudOrigin {
    pub cell_x: i32,
    pub cell_z: i32,
    pub relative_pos: RelativeCameraPos,
    pub offset: [f32; 3],
}

pub fn cloud_origin(
    texture_width: usize,
    texture_height: usize,
    cloud_height: f32,
    camera_pos: [f32; 3],
    game_ticks: i64,
    partial_ticks: f32,
) -> CloudOrigin {
    let relative_bottom_y = cloud_height - camera_pos[1];
    let relative_top_y = relative_bottom_y + CELL_HEIGHT;
    let relative_pos = if relative_top_y < 0.0 {
        RelativeCameraPos::AboveClouds
    } else if relative_bottom_y > 0.0 {
        RelativeCameraPos::BelowClouds
    } else {
        RelativeCameraPos::InsideClouds
    };

    let cloud_offset =
        (game_ticks % (texture_width as i64 * TICKS_PER_CELL)) as f32 + partial_ticks;
    let mut cloud_x = camera_pos[0] + cloud_offset * 0.030_000_001;
    let mut cloud_z = camera_pos[2] + 3.96;
    let texture_width_blocks = texture_width as f32 * CELL_SIZE;
    let texture_height_blocks = texture_height as f32 * CELL_SIZE;
    cloud_x -= (cloud_x / texture_width_blocks).floor() * texture_width_blocks;
    cloud_z -= (cloud_z / texture_height_blocks).floor() * texture_height_blocks;
    let cell_x = (cloud_x / CELL_SIZE).floor() as i32;
    let cell_z = (cloud_z / CELL_SIZE).floor() as i32;
    let x_in_cell = cloud_x - cell_x as f32 * CELL_SIZE;
    let z_in_cell = cloud_z - cell_z as f32 * CELL_SIZE;

    CloudOrigin {
        cell_x,
        cell_z,
        relative_pos,
        offset: [
            camera_pos[0] - x_in_cell,
            relative_bottom_y,
            camera_pos[2] - z_in_cell,
        ],
    }
}

/// Radius in cells needed to cover `range_chunks` chunks of blocks.
pub fn radius_cells(range_chunks: u32) -> i32 {
    let radius_blocks = range_chunks as f32 * 16.0;
    (radius_blocks / CELL_SIZE).ceil() as i32
}

/// Vanilla's cloud mesh bounding radius in world blocks.
pub fn cloud_fog_end(range_chunks: u32) -> f32 {
    (range_chunks as f32 * 16.0).min(CLOUD_FOG_END_DISTANCE)
}

/// Builds the quad soup for one cell-anchored mesh (vanilla `buildMesh`).
pub fn build_cloud_mesh(
    texture: &CloudTexture,
    status: CloudStatus,
    radius: i32,
    center_cell_x: i32,
    center_cell_z: i32,
    relative_pos: RelativeCameraPos,
) -> Vec<CloudVertex> {
    let mut out = Vec::new();
    if status == CloudStatus::Off || radius < 0 {
        return out;
    }
    let extrude = status == CloudStatus::Fancy;

    for ring in 0..=2 * radius {
        for relative_cell_x in -ring..=ring {
            let relative_cell_z = ring - relative_cell_x.abs();
            if relative_cell_z < 0
                || relative_cell_z > radius
                || relative_cell_x * relative_cell_x + relative_cell_z * relative_cell_z
                    > radius * radius
            {
                continue;
            }
            if relative_cell_z != 0 {
                try_build_cell(
                    &mut out,
                    texture,
                    extrude,
                    relative_pos,
                    center_cell_x,
                    center_cell_z,
                    relative_cell_x,
                    -relative_cell_z,
                );
            }
            try_build_cell(
                &mut out,
                texture,
                extrude,
                relative_pos,
                center_cell_x,
                center_cell_z,
                relative_cell_x,
                relative_cell_z,
            );
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn try_build_cell(
    out: &mut Vec<CloudVertex>,
    texture: &CloudTexture,
    extrude: bool,
    relative_pos: RelativeCameraPos,
    center_cell_x: i32,
    center_cell_z: i32,
    x: i32,
    z: i32,
) {
    let cell = texture.cell_at(center_cell_x + x, center_cell_z + z);
    if cell.is_empty() {
        return;
    }
    if !extrude {
        encode_face(out, x, z, 0, FLAG_USE_TOP_COLOR);
        return;
    }

    if relative_pos != RelativeCameraPos::BelowClouds {
        encode_face(out, x, z, 1, 0);
    }
    if relative_pos != RelativeCameraPos::AboveClouds {
        encode_face(out, x, z, 0, 0);
    }
    if cell.neighbor_empty(NORTH_OFFSET) && z > 0 {
        encode_face(out, x, z, 2, 0);
    }
    if cell.neighbor_empty(SOUTH_OFFSET) && z < 0 {
        encode_face(out, x, z, 3, 0);
    }
    if cell.neighbor_empty(WEST_OFFSET) && x > 0 {
        encode_face(out, x, z, 4, 0);
    }
    if cell.neighbor_empty(EAST_OFFSET) && x < 0 {
        encode_face(out, x, z, 5, 0);
    }
    if x.abs() <= 1 && z.abs() <= 1 {
        for direction in 0..6 {
            encode_face(out, x, z, direction, FLAG_INSIDE_FACE);
        }
    }
}

fn encode_face(out: &mut Vec<CloudVertex>, x: i32, z: i32, direction: u32, flags: u32) {
    let dir_flags = (direction & FLAG_MASK_DIR) | flags;
    for corner in QUAD_CORNERS {
        out.push(CloudVertex {
            cell: [x as f32, z as f32],
            dir_flags,
            corner,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2x2 texture: one solid cell, one alpha-9 (empty), one solid, one
    /// alpha-10 (non-empty), all white.
    fn test_texture() -> CloudTexture {
        let mut rgba = vec![0u8; 2 * 2 * 4];
        for pixel in rgba.as_chunks_mut::<4>().0 {
            *pixel = [255, 255, 255, 255];
        }
        rgba[3] = 9; // (0, 0) empty
        rgba[7] = 10; // (1, 0) non-empty at the threshold
        CloudTexture::from_rgba(2, 2, &rgba).unwrap()
    }

    #[test]
    fn from_rgba_packs_neighbor_emptiness_with_wrap() {
        let texture = test_texture();
        // (0, 1) is solid: north (0, 0) is empty; east (1, 1) solid;
        // south (0, 0) wraps to empty; west (1, 1) wraps to solid.
        let (x, y) = (0, 1);
        let cell = texture.cells[x + y * 2];
        assert!(!cell.is_empty());
        assert!(cell.neighbor_empty(NORTH_OFFSET));
        assert!(!cell.neighbor_empty(EAST_OFFSET));
        assert!(cell.neighbor_empty(SOUTH_OFFSET));
        assert!(!cell.neighbor_empty(WEST_OFFSET));
        assert!(texture.cells[0].is_empty());
        assert!(!texture.cells[1].is_empty());
    }

    #[test]
    fn from_rgba_rejects_invalid_size() {
        assert!(CloudTexture::from_rgba(0, 4, &[]).is_none());
        assert!(CloudTexture::from_rgba(2, 2, &[0; 8]).is_none());
    }

    #[test]
    fn fast_status_emits_one_top_colored_quad_per_cell() {
        let texture = test_texture();
        let mesh = build_cloud_mesh(
            &texture,
            CloudStatus::Fast,
            0,
            1,
            0,
            RelativeCameraPos::InsideClouds,
        );
        assert_eq!(mesh.len(), 6); // one non-empty cell
        for vertex in &mesh {
            assert_eq!(vertex.dir_flags, FLAG_USE_TOP_COLOR);
        }
    }

    #[test]
    fn off_status_emits_nothing() {
        let texture = test_texture();
        let mesh = build_cloud_mesh(
            &texture,
            CloudStatus::Off,
            2,
            0,
            0,
            RelativeCameraPos::InsideClouds,
        );
        assert!(mesh.is_empty());
    }

    #[test]
    fn fancy_single_cell_gets_caps_inside_faces_and_boundary_sides() {
        // 1x1 texture: the only cell wraps onto itself, so all four
        // horizontal neighbors are non-empty and no side faces are emitted
        // at the cell's own position; the inside hull is emitted for cells
        // within one of the center.
        let texture = CloudTexture::from_rgba(1, 1, &[255, 255, 255, 255]).unwrap();
        let mesh = build_cloud_mesh(
            &texture,
            CloudStatus::Fancy,
            0,
            0,
            0,
            RelativeCameraPos::InsideClouds,
        );
        // One cell at the center: up + down + six inside faces.
        assert_eq!(mesh.len(), 8 * 6);
        assert!(mesh.iter().any(|v| v.dir_flags == 1)); // up
        assert!(mesh.iter().any(|v| v.dir_flags == 0)); // down
        assert!(mesh.iter().any(|v| v.dir_flags == 16)); // inside down
        assert!(mesh.iter().any(|v| v.dir_flags == 5 | 16)); // inside east
    }

    #[test]
    fn sides_only_face_outward_from_the_center() {
        // 2x2 texture, solid at (0, 0) and (1, 1). Centering on the empty
        // (1, 0) cell, the ring's four non-center solid cells each sit one
        // step out in exactly one axis and emit a side face looking at the
        // empty cell behind them.
        let mut rgba = vec![0_u8; 2 * 2 * 4];
        for &(x, y) in &[(0, 0), (1, 1)] {
            rgba[(y * 2 + x) * 4 + 3] = 255;
        }
        let texture = CloudTexture::from_rgba(2, 2, &rgba).unwrap();
        let mesh = build_cloud_mesh(
            &texture,
            CloudStatus::Fancy,
            1,
            1,
            0,
            RelativeCameraPos::InsideClouds,
        );
        let north = mesh.iter().filter(|v| v.dir_flags == 2).count();
        let south = mesh.iter().filter(|v| v.dir_flags == 3).count();
        let west = mesh.iter().filter(|v| v.dir_flags == 4).count();
        let east = mesh.iter().filter(|v| v.dir_flags == 5).count();
        assert_eq!((north, south, west, east), (6, 6, 6, 6));
        for vertex in &mesh {
            let direction = vertex.dir_flags & FLAG_MASK_DIR;
            let inside = vertex.dir_flags & FLAG_INSIDE_FACE != 0;
            if inside || direction < 2 {
                continue;
            }
            let [x, z] = vertex.cell;
            let outward = match direction {
                2 => z > 0.0,
                3 => z < 0.0,
                4 => x > 0.0,
                _ => x < 0.0,
            };
            assert!(outward, "side face at {:?} points inward", vertex.cell);
        }
    }

    #[test]
    fn caps_face_the_camera_side() {
        let texture = CloudTexture::from_rgba(1, 1, &[255, 255, 255, 255]).unwrap();
        let above = build_cloud_mesh(
            &texture,
            CloudStatus::Fancy,
            0,
            0,
            0,
            RelativeCameraPos::AboveClouds,
        );
        // Camera above the slab sees the cloud top; the bottom cap is
        // skipped (the inside hull still carries a flagged down face).
        assert!(above.iter().any(|v| v.dir_flags == 1));
        assert!(!above.iter().any(|v| v.dir_flags == 0));

        let below = build_cloud_mesh(
            &texture,
            CloudStatus::Fancy,
            0,
            0,
            0,
            RelativeCameraPos::BelowClouds,
        );
        assert!(below.iter().any(|v| v.dir_flags == 0));
        assert!(!below.iter().any(|v| v.dir_flags == 1));
    }

    #[test]
    fn mesh_wraps_cell_indices_around_the_texture() {
        // Center far outside the 2x2 texture: still samples wrapped cells.
        let texture = test_texture();
        let mesh = build_cloud_mesh(
            &texture,
            CloudStatus::Fast,
            0,
            1234,
            -7,
            RelativeCameraPos::BelowClouds,
        );
        assert_eq!(mesh.len(), 6);
        assert_eq!(mesh[0].cell, [0.0, 0.0]);
        assert_eq!(mesh[0].corner, 0);
    }

    #[test]
    fn cloud_origin_matches_vanilla_formulas() {
        let origin = cloud_origin(256, 256, CLOUD_HEIGHT, [100.0, 70.0, -50.0], 0, 0.0);
        // cloudX = 100 -> cell 8; cloudZ = -46.04 wraps to 3025.96 -> cell
        // 252 with 1.96 blocks into it.
        assert_eq!(origin.cell_x, 8);
        assert_eq!(origin.cell_z, 252);
        assert_eq!(origin.relative_pos, RelativeCameraPos::BelowClouds);
        assert!((origin.offset[0] - (100.0 - 4.0)).abs() < 1e-3);
        assert!((origin.offset[2] - (-50.0 - 1.96)).abs() < 1e-3);
        assert!((origin.offset[1] - (CLOUD_HEIGHT - 70.0)).abs() < 1e-6);
    }

    #[test]
    fn cloud_origin_drifts_and_wraps() {
        let a = cloud_origin(256, 256, CLOUD_HEIGHT, [0.0, 100.0, 0.0], 0, 0.0);
        let b = cloud_origin(
            256,
            256,
            CLOUD_HEIGHT,
            [0.0, 100.0, 0.0],
            TICKS_PER_CELL,
            0.0,
        );
        assert_eq!(a.relative_pos, RelativeCameraPos::BelowClouds);
        assert_eq!(a.cell_x, 0);
        assert_eq!(b.cell_x, 1);
        let wrapped = cloud_origin(
            256,
            256,
            CLOUD_HEIGHT,
            [0.0, 100.0, 0.0],
            256 * TICKS_PER_CELL,
            0.0,
        );
        assert_eq!(wrapped.cell_x, a.cell_x);
    }

    #[test]
    fn cloud_origin_handles_negative_wrap() {
        let origin = cloud_origin(
            256,
            256,
            CLOUD_HEIGHT,
            [-100_000.0, 0.0, -100_000.0],
            0,
            0.0,
        );
        // cloudX wraps to 1376 -> cell 114 with 8 blocks; cloudZ wraps to
        // 1379.96 -> cell 114 with 11.96 blocks.
        assert_eq!(origin.cell_x, 114);
        assert_eq!(origin.cell_z, 114);
        assert!((origin.offset[0] - (-100_008.0)).abs() < 1e-3);
        assert!((origin.offset[2] - (-100_011.96)).abs() < 1e-3);
    }

    #[test]
    fn radius_and_fog_end_follow_vanilla_ranges() {
        assert_eq!(radius_cells(2), 3); // 32 blocks -> ceil(2.67)
        assert_eq!(radius_cells(128), 171); // 2048 blocks
        assert_eq!(cloud_fog_end(32), 512.0);
        assert_eq!(cloud_fog_end(128), 2048.0);
        assert_eq!(cloud_fog_end(200), 2048.0);
    }
}
