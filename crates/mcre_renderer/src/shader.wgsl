struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec3<f32>,
    @location(3) light: vec2<f32>,
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

// Vanilla `sample_lightmap` (include/sample_lightmap.glsl): texel center for
// the raw light level pair. A 16x16 texture sampled by index is equivalent
// to vanilla's 256x256 one sampled at `uv / 256 + 0.5 / 16`.
fn sample_lightmap(light: vec2<f32>) -> vec3<f32> {
    let block = i32(clamp(light.x, 0.0, 15.0) + 0.5);
    let sky = i32(clamp(light.y, 0.0, 15.0) + 0.5);
    return textureLoad(lightmap_texture, vec2<i32>(block, sky), 0).rgb;
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = params.view_proj * vec4<f32>(in.position, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    // Vanilla `block.vsh`: `vertexColor = Color * sample_lightmap(...)`,
    // evaluated per vertex so the interpolated color blends lightmapped
    // brightness across the quad face.
    out.light = sample_lightmap(in.light);
    out.distance = distance(in.position, params.camera_pos.xyz);
    out.raw_light = in.light;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var rgb: vec3<f32>;
    if ((params.flags.x & 2u) != 0u) {
        let t = clamp(max(in.raw_light.x, in.raw_light.y) / 15.0, 0.0, 1.0);
        let cool = mix(vec3<f32>(0.0, 0.0, 0.55), vec3<f32>(0.0, 0.85, 0.05), smoothstep(0.0, 0.5, t));
        rgb = mix(cool, vec3<f32>(1.0, 0.05, 0.0), smoothstep(0.5, 1.0, t));
    } else {
        let texel = textureSample(atlas_texture, atlas_sampler, in.uv);
        if (texel.a < 0.5) {
            discard;
        }
        var light = in.light;
        if ((params.flags.x & 1u) != 0u) {
            light = vec3<f32>(1.0);
        }
        rgb = texel.rgb * in.color * light;
    }
    if ((params.flags.x & 4u) == 0u) {
        let fog = smoothstep(params.fog_color_start.w, params.fog_end_sky.x, in.distance);
        rgb = mix(rgb, params.fog_color_start.rgb, fog);
    }
    return vec4<f32>(rgb, 1.0);
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
