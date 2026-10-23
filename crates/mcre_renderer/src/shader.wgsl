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
    /// `x` spherical camera distance, `y` cylindrical camera distance.
    @location(3) distance: vec2<f32>,
    @location(4) raw_light: vec2<f32>,
};

struct Params {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    /// Fog color in `rgb`; `FogEnvironmentalStart` in `a`.
    fog_color_start: vec4<f32>,
    /// `x` environmental end, `y` render-distance start, `z`
    /// render-distance end, `w` cloud fog fade-out distance.
    fog_end_sky: vec4<f32>,
    flags: vec4<u32>,
    /// `x` texcoord shrink epsilon, `y` fog density.
    fog_misc: vec4<f32>,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var atlas_texture: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;
@group(0) @binding(3) var lightmap_texture: texture_2d<f32>;
@group(0) @binding(4) var<storage, read> section_table: array<vec4<f32>>;
@group(0) @binding(5) var lightmap_sampler: sampler;

/// Vanilla `linear_fog_value`.
fn linear_fog_value(distance: f32, start: f32, end: f32) -> f32 {
    if (distance <= start) {
        return 0.0;
    }
    if (distance >= end) {
        return 1.0;
    }
    return (distance - start) / (end - start);
}

fn fog_value(in: VertexOutput) -> f32 {
    let environmental = linear_fog_value(in.distance.x, params.fog_color_start.w, params.fog_end_sky.x);
    let render_distance = linear_fog_value(in.distance.y, params.fog_end_sky.y, params.fog_end_sky.z);
    return max(environmental, render_distance);
}

@vertex
fn vs_main(in: VertexInput, @builtin(instance_index) instance: u32) -> VertexOutput {
    var out: VertexOutput;
    let section_origin = section_table[instance].xyz;
    let world_pos = decode_position(in.position) + section_origin;
    out.clip_position = params.view_proj * vec4<f32>(world_pos, 1.0);
    out.uv = decode_texcoord(in.tex) + decode_texcoord_bias(in.tex) * params.fog_misc.x;
    let light_uv = decode_lightmap_uv(in.light_data.xy);
    var light = textureSampleLevel(lightmap_texture, lightmap_sampler, light_uv, 0.0).rgb;
    if ((params.flags.x & 1u) != 0u) {
        light = vec3<f32>(1.0);
    }
    // Vanilla `terrain.vsh`: the lightmap multiplies the vertex color before
    // interpolation, so the product (not each factor) is interpolated.
    out.color = in.color.rgb * light;
    let relative = world_pos - params.camera_pos.xyz;
    out.distance = vec2<f32>(length(relative), max(length(relative.xz), abs(relative.y)));
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
        rgb = texel.rgb * in.color;
        alpha = texel.a;
    }
    if ((params.flags.x & 4u) == 0u) {
        let fog = fog_value(in) * params.fog_misc.y;
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
