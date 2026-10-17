// Port of vanilla `assets/minecraft/shaders/core/clouds.vsh` + `clouds.fsh`.
// One quad is 6 vertices with the `sharedSequentialQuad` corner pattern
// (0, 1, 2, 2, 3, 0); each vertex carries its cell and the packed
// direction/flags word from `CloudRenderer.encodeFace`.

struct Params {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    fog_color_start: vec4<f32>,
    /// Fog end distance in `x`, cloud fade-out distance in `y`.
    fog_end_sky: vec4<f32>,
    flags: vec4<u32>,
};

@group(0) @binding(0) var<uniform> params: Params;

struct CloudInfo {
    color: vec4<f32>,
    offset: vec4<f32>,
    cell_size: vec4<f32>,
};

@group(1) @binding(0) var<uniform> cloud_info: CloudInfo;

struct CloudInput {
    @location(0) cell: vec2<f32>,
    @location(1) @interpolate(flat) dir_flags: u32,
    @location(2) @interpolate(flat) corner: u32,
};

struct CloudOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) distance: f32,
};

const FLAG_MASK_DIR: u32 = 7u;
const FLAG_INSIDE_FACE: u32 = 16u;
const FLAG_USE_TOP_COLOR: u32 = 32u;

const PACKED_X: u32 = 0xF03CC3u;
const PACKED_Y: u32 = 0x6666F0u;
const PACKED_Z: u32 = 0xC3F066u;

const FACE_COLORS = array<vec4<f32>, 6>(
    vec4<f32>(0.7, 0.7, 0.7, 1.0),
    vec4<f32>(1.0, 1.0, 1.0, 1.0),
    vec4<f32>(0.8, 0.8, 0.8, 1.0),
    vec4<f32>(0.8, 0.8, 0.8, 1.0),
    vec4<f32>(0.9, 0.9, 0.9, 1.0),
    vec4<f32>(0.9, 0.9, 0.9, 1.0),
);

fn packed_bit(packed: u32, index: u32) -> f32 {
    return f32((packed >> index) & 1u);
}

fn corner_vertex(index: u32) -> vec3<f32> {
    return vec3<f32>(
        packed_bit(PACKED_X, index),
        packed_bit(PACKED_Y, index),
        packed_bit(PACKED_Z, index),
    );
}

@vertex
fn vs_clouds(in: CloudInput) -> CloudOutput {
    let direction = in.dir_flags & FLAG_MASK_DIR;
    let inside_face = (in.dir_flags & FLAG_INSIDE_FACE) == FLAG_INSIDE_FACE;
    let use_top_color = (in.dir_flags & FLAG_USE_TOP_COLOR) == FLAG_USE_TOP_COLOR;
    var corner = in.corner;
    if inside_face {
        corner = 3u - corner;
    }
    let face_vertex = corner_vertex(direction * 4u + corner);
    let pos = (face_vertex + vec3<f32>(in.cell.x, 0.0, in.cell.y)) * cloud_info.cell_size.xyz
        + cloud_info.offset.xyz;
    let face_color = select(FACE_COLORS[direction], FACE_COLORS[1], use_top_color);

    var out: CloudOutput;
    out.clip_position = params.view_proj * vec4<f32>(pos, 1.0);
    out.color = face_color * cloud_info.color;
    out.distance = length(pos - params.camera_pos.xyz);
    return out;
}

@fragment
fn fs_clouds(in: CloudOutput) -> @location(0) vec4<f32> {
    var color = in.color;
    let fog = clamp(in.distance / params.fog_end_sky.y, 0.0, 1.0);
    color.a *= 1.0 - fog;
    return color;
}
