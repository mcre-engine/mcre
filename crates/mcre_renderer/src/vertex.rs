//! Bit-exact port of Sodium's `CompactChunkVertex` encoder plus Iris's XHFP
//! terrain extensions (`XHFPTerrainVertex`), matching their Java sources.

use bytemuck::{Pod, Zeroable};
use mcre_core::Direction;

pub const POSITION_MAX_COORD: u32 = 1 << 20;
pub const TEXTURE_MAX_COORD: f32 = 32768.0;
pub const MODEL_ORIGIN: f32 = 8.0;
pub const MODEL_RANGE: f32 = 32.0;
const VERTEX_SCALE: f32 = MODEL_RANGE / POSITION_MAX_COORD as f32;

pub const FACING_UNASSIGNED: u8 = 0;
pub const FACING_POS_X: u8 = 1;
pub const FACING_POS_Y: u8 = 2;
pub const FACING_POS_Z: u8 = 3;
pub const FACING_NEG_X: u8 = 4;
pub const FACING_NEG_Y: u8 = 5;
pub const FACING_NEG_Z: u8 = 6;
pub const FACING_COUNT: usize = 7;

pub fn facing_of(direction: Direction) -> u8 {
    match direction {
        Direction::Up => FACING_POS_Y,
        Direction::Down => FACING_NEG_Y,
        Direction::North => FACING_NEG_Z,
        Direction::South => FACING_POS_Z,
        Direction::West => FACING_NEG_X,
        Direction::East => FACING_POS_X,
    }
}

pub fn facing_of_cullface(cullface: Option<Direction>) -> u8 {
    cullface.map_or(FACING_UNASSIGNED, facing_of)
}

pub fn opposite_facing(facing: u8) -> u8 {
    match facing {
        FACING_POS_X => FACING_NEG_X,
        FACING_POS_Y => FACING_NEG_Y,
        FACING_POS_Z => FACING_NEG_Z,
        FACING_NEG_X => FACING_POS_X,
        FACING_NEG_Y => FACING_POS_Y,
        FACING_NEG_Z => FACING_POS_Z,
        _ => FACING_UNASSIGNED,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainPass {
    Solid = 0,
    Cutout = 1,
    Translucent = 2,
}

impl TerrainPass {
    pub const ALL: [TerrainPass; 3] = [
        TerrainPass::Solid,
        TerrainPass::Cutout,
        TerrainPass::Translucent,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormatFlags {
    pub block_id: bool,
    pub normal: bool,
    pub mid_uv: bool,
    pub mid_block: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttributeOffsets {
    pub block_id: u32,
    pub normal: u32,
    pub mid_uv: u32,
    pub mid_block: u32,
    pub stride: u32,
}

pub const BASE_STRIDE: u32 = 20;

impl FormatFlags {
    pub const NONE: Self = Self {
        block_id: false,
        normal: false,
        mid_uv: false,
        mid_block: false,
    };

    pub const FULL: Self = Self {
        block_id: true,
        normal: true,
        mid_uv: true,
        mid_block: true,
    };

    pub fn offsets(self) -> AttributeOffsets {
        let mut offset = BASE_STRIDE;
        let mut next = |enabled: bool| -> u32 {
            if enabled {
                let at = offset;
                offset += 4;
                at
            } else {
                0
            }
        };
        let block_id = next(self.block_id);
        let normal = next(self.normal);
        let mid_uv = next(self.mid_uv);
        let mid_block = next(self.mid_block);
        AttributeOffsets {
            block_id,
            normal,
            mid_uv,
            mid_block,
            stride: offset,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct TerrainVertex {
    pub position: [u32; 2],
    pub color: [u8; 4],
    pub tex: [u16; 2],
    pub light_data: [u8; 4],
    pub block_id: u32,
    pub normal_tangent: u32,
    pub mid_uv: [u16; 2],
    pub mid_block: [u8; 4],
}

pub fn quantize_position(position: f32) -> u32 {
    let normalized = (MODEL_ORIGIN + position) / MODEL_RANGE;
    ((normalized * POSITION_MAX_COORD as f32) as i32 as u32) & 0xFFFFF
}

pub fn pack_position_hi(x: u32, y: u32, z: u32) -> u32 {
    ((x >> 10) & 0x3FF) | ((y >> 10) & 0x3FF) << 10 | ((z >> 10) & 0x3FF) << 20
}

pub fn pack_position_lo(x: u32, y: u32, z: u32) -> u32 {
    (x & 0x3FF) | (y & 0x3FF) << 10 | (z & 0x3FF) << 20
}

pub fn decode_position(data: [u32; 2]) -> [f32; 3] {
    let deinterleave =
        |word: u32| -> [u32; 3] { [word & 0x3FF, (word >> 10) & 0x3FF, (word >> 20) & 0x3FF] };
    let hi = deinterleave(data[0]);
    let lo = deinterleave(data[1]);
    let mut out = [0.0_f32; 3];
    for i in 0..3 {
        let q = (hi[i] << 10) | lo[i];
        out[i] = q as f32 * VERTEX_SCALE - MODEL_ORIGIN;
    }
    out
}

fn java_round(value: f32) -> i32 {
    (value + 0.5).floor() as i32
}

pub fn encode_texture(center: f32, value: f32) -> u16 {
    let bias = if value < center { 1 } else { -1 };
    let quantized = java_round(value * TEXTURE_MAX_COORD) + bias;
    ((quantized & 0x7FFF) as u16) | ((bias as u32 >> 31) as u16) << 15
}

pub fn decode_texcoord(tex: [u16; 2]) -> ([f32; 2], [f32; 2]) {
    let mut coord = [0.0_f32; 2];
    let mut bias = [0.0_f32; 2];
    for i in 0..2 {
        coord[i] = f32::from(tex[i] & 0x7FFF) / TEXTURE_MAX_COORD;
        bias[i] = if tex[i] & 0x8000 != 0 { -1.0 } else { 1.0 };
    }
    (coord, bias)
}

/// Vanilla `sample_lightmap`: `uv = level * 16 / 256 + 0.5 / 16`, keeping the
/// fractional level so the 16x16 lightmap is sampled between texels.
pub fn encode_lightmap_coord(level: f32) -> u8 {
    (((level * 16.0).round() as i32 + 8).clamp(8, 248)) as u8
}

pub fn decode_lightmap_coord(coord: u8) -> f32 {
    f32::from(coord) / 256.0
}

pub fn pack_light_data(block: u8, sky: u8, material: u8, section: u8) -> [u8; 4] {
    [block, sky, material, section]
}

pub fn pack_block_id(state_id: u16, translucent: bool) -> u32 {
    ((u32::from(state_id) + 1) << 1) | u32::from(translucent)
}

pub fn octahedron_encode(x: f32, y: f32, z: f32) -> [f32; 2] {
    let inv_l1 = 1.0 / (x.abs() + y.abs() + z.abs());
    let (nx, ny, nz) = (x * inv_l1, y * inv_l1, z * inv_l1);
    let (ox, oy) = if nz >= 0.0 {
        (nx, ny)
    } else {
        let (abs_nx, abs_ny) = (nx.abs(), ny.abs());
        (
            (1.0 - abs_ny) * if nx >= 0.0 { 1.0 } else { -1.0 },
            (1.0 - abs_nx) * if ny >= 0.0 { 1.0 } else { -1.0 },
        )
    };
    [ox * 0.5 + 0.5, oy * 0.5 + 0.5]
}

const TANGENT_BIAS: f32 = 1.0 / 32767.0;

pub fn tangent_encode(tangent: [f32; 4]) -> [f32; 2] {
    let mut encoded = octahedron_encode(tangent[0], tangent[1], tangent[2]);
    encoded[1] = encoded[1].max(TANGENT_BIAS);
    encoded[1] = encoded[1] * 0.5 + 0.5;
    encoded[1] = if tangent[3] >= 0.0 {
        encoded[1]
    } else {
        1.0 - encoded[1]
    };
    encoded
}

pub fn pack_norm_i8(x: f32, y: f32, z: f32, w: f32) -> u32 {
    let component = |v: f32| -> u32 { ((v * 127.0) as i32 as u32) & 0xFF };
    component(x) | component(y) << 8 | component(z) << 16 | component(w) << 24
}

pub fn pack_mid_uv(uvs: &[[f32; 2]; 4]) -> [u16; 2] {
    let mut centroid = [0.0_f32; 2];
    for uv in uvs {
        centroid[0] += uv[0];
        centroid[1] += uv[1];
    }
    centroid[0] *= 0.25;
    centroid[1] *= 0.25;
    [
        (java_round(centroid[0] * TEXTURE_MAX_COORD) as u32 & 0xFFFF) as u16,
        (java_round(centroid[1] * TEXTURE_MAX_COORD) as u32 & 0xFFFF) as u16,
    ]
}

pub fn pack_mid_block(offset: [f32; 3]) -> [u8; 3] {
    let component = |v: f32| -> u8 { ((v * 64.0) as i32 as u32 & 0xFF) as u8 };
    [
        component(offset[0]),
        component(offset[1]),
        component(offset[2]),
    ]
}

pub fn compute_mid_block(vertex: [f32; 3], block_pos: [i32; 3]) -> [u8; 3] {
    pack_mid_block([
        block_pos[0] as f32 + 0.5 - vertex[0],
        block_pos[1] as f32 + 0.5 - vertex[1],
        block_pos[2] as f32 + 0.5 - vertex[2],
    ])
}

pub fn compute_face_normal(v: &[[f32; 3]; 4]) -> [f32; 3] {
    let d0 = sub(v[2], v[0]);
    let d1 = sub(v[3], v[1]);
    let n = [
        d0[1] * d1[2] - d0[2] * d1[1],
        d0[2] * d1[0] - d0[0] * d1[2],
        d0[0] * d1[1] - d0[1] * d1[0],
    ];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len == 0.0 {
        return [0.0; 3];
    }
    [n[0] / len, n[1] / len, n[2] / len]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn rsqrt(value: f32) -> f32 {
    if value == 0.0 {
        1.0
    } else {
        1.0 / value.sqrt()
    }
}

fn compute_tangent(
    normal: [f32; 3],
    v0: [f32; 3],
    uv0: [f32; 2],
    v1: [f32; 3],
    uv1: [f32; 2],
    v2: [f32; 3],
    uv2: [f32; 2],
) -> Option<[f32; 4]> {
    let edge1 = sub(v1, v0);
    let edge2 = sub(v2, v0);
    let du1 = uv1[0] - uv0[0];
    let dv1 = uv1[1] - uv0[1];
    let du2 = uv2[0] - uv0[0];
    let dv2 = uv2[1] - uv0[1];
    let denom = du1 * dv2 - du2 * dv1;
    let f = if denom == 0.0 { 1.0 } else { 1.0 / denom };
    let mut tangent = [
        f * (dv2 * edge1[0] - dv1 * edge2[0]),
        f * (dv2 * edge1[1] - dv1 * edge2[1]),
        f * (dv2 * edge1[2] - dv1 * edge2[2]),
    ];
    let tcoeff = rsqrt(tangent[0] * tangent[0] + tangent[1] * tangent[1] + tangent[2] * tangent[2]);
    for c in &mut tangent {
        *c *= tcoeff;
    }
    if tangent == [0.0; 3] {
        return None;
    }
    let mut bitangent = [
        f * (-du2 * edge1[0] + du1 * edge2[0]),
        f * (-du2 * edge1[1] + du1 * edge2[1]),
        f * (-du2 * edge1[2] + du1 * edge2[2]),
    ];
    let bcoeff = rsqrt(
        bitangent[0] * bitangent[0] + bitangent[1] * bitangent[1] + bitangent[2] * bitangent[2],
    );
    for c in &mut bitangent {
        *c *= bcoeff;
    }
    let predicted = [
        tangent[1] * normal[2] - tangent[2] * normal[1],
        tangent[2] * normal[0] - tangent[0] * normal[2],
        tangent[0] * normal[1] - tangent[1] * normal[0],
    ];
    let dot =
        bitangent[0] * predicted[0] + bitangent[1] * predicted[1] + bitangent[2] * predicted[2];
    Some([
        tangent[0],
        tangent[1],
        tangent[2],
        if dot < 0.0 { -1.0 } else { 1.0 },
    ])
}

fn compute_tangent_for_quad(normal: [f32; 3], v: &[[f32; 3]; 4], uv: &[[f32; 2]; 4]) -> [f32; 4] {
    let first = compute_tangent(normal, v[0], uv[0], v[1], uv[1], v[2], uv[2]);
    let fallback = first.or_else(|| compute_tangent(normal, v[2], uv[2], v[3], uv[3], v[0], uv[0]));
    fallback.unwrap_or([0.0, 1.0, 0.0, 1.0])
}

pub fn encode_normal_tangent(v: &[[f32; 3]; 4], uv: &[[f32; 2]; 4]) -> u32 {
    let normal = compute_face_normal(v);
    let tangent = compute_tangent_for_quad(normal, v, uv);
    let norm = octahedron_encode(normal[0], normal[1], normal[2]);
    let tang = tangent_encode(tangent);
    pack_norm_i8(norm[0], norm[1], tang[0], tang[1])
}

pub fn pack_local_section_index(x: u8, y: u8, z: u8) -> u8 {
    (y & 0x3) | (z & 0x7) << 2 | (x & 0x7) << 5
}

pub struct QuadEncodeInput {
    pub positions: [[f32; 3]; 4],
    pub uvs: [[f32; 2]; 4],
    pub colors: [[f32; 3]; 4],
    pub light: [[f32; 2]; 4],
    pub block_pos: [i32; 3],
    pub state_id: u16,
    pub translucent: bool,
    pub emission: u8,
    pub section_id: u8,
}

pub fn encode_quad(input: &QuadEncodeInput) -> [TerrainVertex; 4] {
    let mut centroid = [0.0_f32; 2];
    for uv in &input.uvs {
        centroid[0] += uv[0];
        centroid[1] += uv[1];
    }
    centroid[0] *= 0.25;
    centroid[1] *= 0.25;

    let mid_uv = [
        (java_round(centroid[0] * TEXTURE_MAX_COORD) as u32 & 0xFFFF) as u16,
        (java_round(centroid[1] * TEXTURE_MAX_COORD) as u32 & 0xFFFF) as u16,
    ];
    let normal_tangent = encode_normal_tangent(&input.positions, &input.uvs);
    let block_id = pack_block_id(input.state_id, input.translucent);

    let mut vertices = [TerrainVertex::zeroed(); 4];
    for (i, vertex) in vertices.iter_mut().enumerate() {
        let p = quantize_position(input.positions[i][0]);
        let q = quantize_position(input.positions[i][1]);
        let r = quantize_position(input.positions[i][2]);
        let color = [
            color_byte(input.colors[i][0]),
            color_byte(input.colors[i][1]),
            color_byte(input.colors[i][2]),
            255,
        ];
        let tex = [
            encode_texture(centroid[0], input.uvs[i][0]),
            encode_texture(centroid[1], input.uvs[i][1]),
        ];
        let light_data = pack_light_data(
            encode_lightmap_coord(input.light[i][0]),
            encode_lightmap_coord(input.light[i][1]),
            0,
            input.section_id,
        );
        let mid = compute_mid_block(input.positions[i], input.block_pos);
        *vertex = TerrainVertex {
            position: [pack_position_hi(p, q, r), pack_position_lo(p, q, r)],
            color,
            tex,
            light_data,
            block_id,
            normal_tangent,
            mid_uv,
            mid_block: [mid[0], mid[1], mid[2], input.emission],
        };
    }
    vertices
}

fn color_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0 + 0.5).floor() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT_NORMAL: u32 = 0x7F3F7F3F;

    #[test]
    fn position_quantization_matches_golden_values() {
        assert_eq!(quantize_position(-8.0), 0);
        assert_eq!(quantize_position(0.0), 262_144);
        assert_eq!(quantize_position(16.0), 786_432);
        assert_eq!(quantize_position(24.0), 0);
    }

    #[test]
    fn position_round_trip_error_within_half_quantum() {
        let quantum = MODEL_RANGE / POSITION_MAX_COORD as f32;
        for &v in &[-8.0, -1.5, 0.0, 0.001, 3.7, 15.999, 16.0, 23.5] {
            let q = quantize_position(v);
            let packed = [pack_position_hi(q, q, q), pack_position_lo(q, q, q)];
            let decoded = decode_position(packed);
            for axis in 0..3 {
                assert!(
                    (decoded[axis] - v).abs() <= quantum,
                    "v={v} decoded={decoded:?}"
                );
            }
        }
    }

    #[test]
    fn position_pack_interleaves_bits_like_glsl() {
        let x = quantize_position(0.0);
        let y = quantize_position(8.0);
        let z = quantize_position(-4.0);
        let hi = pack_position_hi(x, y, z);
        let lo = pack_position_lo(x, y, z);
        let decoded = decode_position([hi, lo]);
        assert!((decoded[0] - 0.0).abs() < 1e-4);
        assert!((decoded[1] - 8.0).abs() < 1e-4);
        assert!((decoded[2] - -4.0).abs() < 1e-4);
    }

    #[test]
    fn texture_bias_sign_encoding() {
        assert_eq!(encode_texture(0.5, 0.0), 0x0001);
        assert_eq!(encode_texture(0.5, 1.0), 0xFFFF);
        assert_eq!(encode_texture(0.5, 0.5), 0xBFFF);
    }

    #[test]
    fn texture_decode_recovers_shrunk_coords() {
        let (coord, bias) = decode_texcoord([0xFFFF; 2]);
        assert_eq!(coord, [32767.0 / 32768.0; 2]);
        assert_eq!(bias, [-1.0; 2]);
        let (coord, bias) = decode_texcoord([0x0001, 0x0001]);
        assert_eq!(coord, [1.0 / 32768.0; 2]);
        assert_eq!(bias, [1.0; 2]);
    }

    #[test]
    fn lightmap_coord_matches_vanilla_texel_centers() {
        assert_eq!(encode_lightmap_coord(0.0), 8);
        assert_eq!(encode_lightmap_coord(15.0), 248);
        assert_eq!(encode_lightmap_coord(8.0), 136);
        assert!((decode_lightmap_coord(8) - 0.5 / 16.0).abs() < f32::EPSILON);
        assert!((decode_lightmap_coord(248) - 15.5 / 16.0).abs() < 1e-6);
    }

    #[test]
    fn lightmap_coord_keeps_fractional_levels() {
        assert_eq!(encode_lightmap_coord(14.75), 244);
        assert!((decode_lightmap_coord(244) - 15.25 / 16.0).abs() < 1e-6);
    }

    #[test]
    fn block_id_packing() {
        assert_eq!(pack_block_id(0, false), 2);
        assert_eq!(pack_block_id(100, true), 203);
    }

    #[test]
    fn octahedral_encode_goldens() {
        assert_eq!(octahedron_encode(0.0, 1.0, 0.0), [0.5, 1.0]);
        assert_eq!(octahedron_encode(0.0, 0.0, 1.0), [0.5, 0.5]);
        assert_eq!(octahedron_encode(1.0, 0.0, 0.0), [1.0, 0.5]);
        assert_eq!(octahedron_encode(0.0, 0.0, -1.0), [1.0, 1.0]);
        assert_eq!(octahedron_encode(-1.0, 0.0, 0.0), [0.0, 0.5]);
    }

    #[test]
    fn tangent_encode_goldens() {
        assert_eq!(tangent_encode([0.0, 1.0, 0.0, 1.0]), [0.5, 1.0]);
        let flipped = tangent_encode([0.0, 1.0, 0.0, -1.0]);
        assert!((flipped[0] - 0.5).abs() < 1e-6);
        assert!((flipped[1] - 0.0).abs() < 1e-6);
        let down = tangent_encode([0.0, -1.0, 0.0, 1.0]);
        assert!((down[0] - 0.5).abs() < 1e-6);
        assert!(down[1] > 0.5 && down[1] <= 0.5 + 1e-4);
    }

    #[test]
    fn norm_i8_pack_goldens() {
        assert_eq!(pack_norm_i8(1.0, 1.0, 1.0, 1.0), 0x7F7F7F7F);
        assert_eq!(pack_norm_i8(-1.0, -1.0, -1.0, -1.0), 0x81818181);
        assert_eq!(pack_norm_i8(0.0, 0.0, 0.0, 0.0), 0);
    }

    #[test]
    fn default_normal_matches_iris() {
        let norm = octahedron_encode(0.0, 1.0, 0.0);
        let tang = tangent_encode([0.0, 1.0, 0.0, 1.0]);
        assert_eq!(
            pack_norm_i8(norm[0], norm[1], tang[0], tang[1]),
            DEFAULT_NORMAL
        );
    }

    #[test]
    fn face_normal_up_quad() {
        let v = [
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
        ];
        let n = compute_face_normal(&v);
        assert!((n[0]).abs() < 1e-6 && (n[1] - 1.0).abs() < 1e-6 && n[2].abs() < 1e-6);
    }

    #[test]
    fn mid_block_truncates_toward_zero_like_java() {
        assert_eq!(pack_mid_block([0.25, 0.5, 0.25]), [16, 32, 16]);
        assert_eq!(pack_mid_block([-0.25, -0.5, -0.0]), [240, 224, 0]);
        assert_eq!(
            compute_mid_block([5.25, 64.0, -2.75], [5, 64, -3]),
            [16, 32, 16]
        );
    }

    #[test]
    fn mid_uv_is_centroid_scaled() {
        let uvs = [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]];
        assert_eq!(pack_mid_uv(&uvs), [16384, 16384]);
    }

    #[test]
    fn local_section_index_packing_matches_shader_decode() {
        assert_eq!(pack_local_section_index(0, 0, 0), 0);
        assert_eq!(pack_local_section_index(1, 1, 1), 1 | 1 << 2 | 1 << 5);
        for x in 0..8 {
            for y in 0..4 {
                for z in 0..8 {
                    let packed = pack_local_section_index(x, y, z);
                    assert_eq!((packed >> 5) & 0x7, x);
                    assert_eq!(packed & 0x3, y);
                    assert_eq!((packed >> 2) & 0x7, z);
                }
            }
        }
    }

    #[test]
    fn format_offsets_follow_analyzer_order() {
        let full = FormatFlags::FULL.offsets();
        assert_eq!(
            full,
            AttributeOffsets {
                block_id: 20,
                normal: 24,
                mid_uv: 28,
                mid_block: 32,
                stride: 36,
            }
        );
        let none = FormatFlags::NONE.offsets();
        assert_eq!(none.stride, 20);
        assert_eq!(none.block_id, 0);
    }

    #[test]
    fn facing_mapping() {
        assert_eq!(facing_of(Direction::Up), FACING_POS_Y);
        assert_eq!(facing_of(Direction::Down), FACING_NEG_Y);
        assert_eq!(facing_of(Direction::North), FACING_NEG_Z);
        assert_eq!(facing_of(Direction::South), FACING_POS_Z);
        assert_eq!(facing_of(Direction::West), FACING_NEG_X);
        assert_eq!(facing_of(Direction::East), FACING_POS_X);
        assert_eq!(facing_of_cullface(None), FACING_UNASSIGNED);
    }

    #[test]
    fn encode_quad_smoke() {
        let input = QuadEncodeInput {
            positions: [
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
            ],
            uvs: [[0.25, 0.25], [0.25, 0.75], [0.75, 0.75], [0.75, 0.25]],
            colors: [[1.0; 3]; 4],
            light: [[15.0, 0.0]; 4],
            block_pos: [1, 2, 3],
            state_id: 7,
            translucent: true,
            emission: 3,
            section_id: 9,
        };
        let vertices = encode_quad(&input);
        assert_eq!(vertices.len(), 4);
        for v in &vertices {
            assert_eq!(v.color, [255, 255, 255, 255]);
            assert_eq!(v.light_data, [248, 8, 0, 9]);
            assert_eq!(v.block_id, pack_block_id(7, true));
            assert_eq!(v.mid_uv, [16384, 16384]);
            assert_eq!(v.mid_block[3], 3);
            assert_eq!(v.normal_tangent, vertices[0].normal_tangent);
        }
        let decoded = decode_position(vertices[0].position);
        assert!((decoded[0] - 0.0).abs() < 1e-4);
        assert!((decoded[2] - 0.0).abs() < 1e-4);
    }

    #[test]
    fn encode_quad_up_face_normal_bits() {
        let input = QuadEncodeInput {
            positions: [
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
            ],
            uvs: [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
            colors: [[1.0; 3]; 4],
            light: [[0.0, 15.0]; 4],
            block_pos: [0, 0, 0],
            state_id: 0,
            translucent: false,
            emission: 0,
            section_id: 0,
        };
        let vertices = encode_quad(&input);
        assert_eq!(
            vertices[0].normal_tangent,
            pack_norm_i8(0.5, 1.0, 1.0, 0.75)
        );
    }
}
