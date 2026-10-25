struct Params {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    fog_color_start: vec4<f32>,
    fog_end_sky: vec4<f32>,
    flags: vec4<u32>,
    fog_misc: vec4<f32>,
    tex_size: vec4<f32>,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var atlas_texture: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;
@group(0) @binding(3) var lightmap_texture: texture_2d<f32>;
@group(0) @binding(5) var lightmap_sampler: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) light: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) distance: vec2<f32>,
};

@vertex
fn vs_particle(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let relative = input.position - floor(params.camera_pos.xyz);
    output.clip_position = params.view_proj * vec4<f32>(relative, 1.0);
    output.uv = input.uv;
    let light_uv = (input.light * 16.0 + vec2<f32>(8.0)) / 256.0;
    var light = textureSampleLevel(lightmap_texture, lightmap_sampler, light_uv, 0.0).rgb;
    if ((params.flags.x & 1u) != 0u) {
        light = vec3<f32>(1.0);
    }
    output.color = vec4<f32>(input.color.rgb * light, input.color.a);
    let camera_fraction = params.camera_pos.xyz - floor(params.camera_pos.xyz);
    let camera_relative = relative - camera_fraction;
    output.distance = vec2<f32>(
        length(camera_relative),
        max(length(camera_relative.xz), abs(camera_relative.y)),
    );
    return output;
}

fn linear_fog_value(distance: f32, start: f32, end: f32) -> f32 {
    if (distance <= start) {
        return 0.0;
    }
    if (distance >= end) {
        return 1.0;
    }
    return (distance - start) / (end - start);
}

@fragment
fn fs_particle(input: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSampleLevel(atlas_texture, atlas_sampler, input.uv, 0.0);
    var color = texel.rgb * input.color.rgb;
    if ((params.flags.x & 4u) == 0u) {
        let environmental = linear_fog_value(
            input.distance.x,
            params.fog_color_start.w,
            params.fog_end_sky.x,
        );
        let render_distance = linear_fog_value(
            input.distance.y,
            params.fog_end_sky.y,
            params.fog_end_sky.z,
        );
        let fog = max(environmental, render_distance) * params.fog_misc.y;
        color = mix(color, params.fog_color_start.rgb, fog);
    }
    return vec4<f32>(color, texel.a * input.color.a);
}
