// Terrain shader: Sodium `CompactChunkVertex` + Iris XHFP decode.
// `terrain_vertex.wgsl` (packed decoders) is concatenated in front of this
// file at pipeline-build time.

struct VertexInput {
    @location(0) position: vec2<u32>,
    @location(1) color: vec4<f32>,
    @location(2) tex: vec2<u32>,
    @location(3) light_data: vec4<u32>,
    @location(4) block_id: u32,
    @location(5) normal_tangent: vec4<f32>,
    @location(6) mid_uv: vec2<u32>,
    @location(7) mid_block: vec4<i32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec3<f32>,
    @location(2) light: vec3<f32>,
    @location(3) distance: f32,
    @location(4) raw_light: vec2<f32>,
};

struct Params {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    fog_color_start: vec4<f32>,
    fog_end_sky: vec4<f32>,
    flags: vec4<u32>,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var atlas_texture: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;
@group(0) @binding(3) var lightmap_texture: texture_2d<f32>;
@group(0) @binding(4) var<storage, read> section_table: array<vec4<f32>>;
@group(0) @binding(5) var lightmap_sampler: sampler;

@vertex
fn vs_main(in: VertexInput, @builtin(instance_index) instance: u32) -> VertexOutput {
    var out: VertexOutput;
    let section_origin = section_table[instance].xyz;
    let world_pos = decode_position(in.position) + section_origin;
    out.clip_position = params.view_proj * vec4<f32>(world_pos, 1.0);
    out.uv = decode_texcoord(in.tex) + decode_texcoord_bias(in.tex) * params.fog_end_sky.z;
    out.color = in.color.rgb;
    let light_uv = decode_lightmap_uv(in.light_data.xy);
    out.light = textureSampleLevel(lightmap_texture, lightmap_sampler, light_uv, 0.0).rgb;
    out.distance = distance(world_pos, params.camera_pos.xyz);
    out.raw_light = light_uv;
    return out;
}

fn terrain_color(in: VertexOutput, cutout: bool) -> vec4<f32> {
    var rgb: vec3<f32>;
    var alpha = 1.0;
    if ((params.flags.x & 2u) != 0u) {
        let t = clamp(max(in.raw_light.x, in.raw_light.y), 0.0, 1.0);
        let cool = mix(vec3<f32>(0.0, 0.0, 0.55), vec3<f32>(0.0, 0.85, 0.05), smoothstep(0.0, 0.5, t));
        rgb = mix(cool, vec3<f32>(1.0, 0.05, 0.0), smoothstep(0.5, 1.0, t));
    } else {
        let texel = textureSample(atlas_texture, atlas_sampler, in.uv);
        if (cutout && texel.a < 0.5) {
            discard;
        }
        var light = in.light;
        if ((params.flags.x & 1u) != 0u) {
            light = vec3<f32>(1.0);
        }
        rgb = texel.rgb * in.color * light;
        alpha = texel.a;
    }
    if ((params.flags.x & 4u) == 0u) {
        let fog = smoothstep(params.fog_color_start.w, params.fog_end_sky.x, in.distance);
        rgb = mix(rgb, params.fog_color_start.rgb, fog);
    }
    return vec4<f32>(rgb, alpha);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return terrain_color(in, true);
}

@fragment
fn fs_translucent(in: VertexOutput) -> @location(0) vec4<f32> {
    return terrain_color(in, false);
}

struct LineOutput {
    @builtin(position) clip_position: vec4<f32>,
};

@vertex
fn vs_line(@location(0) position: vec3<f32>) -> LineOutput {
    var out: LineOutput;
    out.clip_position = params.view_proj * vec4<f32>(position, 1.0);
    return out;
}

@fragment
fn fs_line() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 0.1, 0.1, 1.0);
}
