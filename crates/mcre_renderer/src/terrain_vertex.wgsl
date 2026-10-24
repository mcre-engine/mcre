const POSITION_MAX_COORD: u32 = 1u << 20u;
const TEXTURE_MAX_COORD: f32 = 32768.0;
const VERTEX_SCALE: f32 = 32.0 / f32(POSITION_MAX_COORD);
const VERTEX_OFFSET: f32 = -8.0;

fn deinterleave_u20x3(data: vec2<u32>) -> vec3<u32> {
    let hi = (vec3<u32>(data.x) >> vec3<u32>(0u, 10u, 20u)) & vec3<u32>(0x3FFu);
    let lo = (vec3<u32>(data.y) >> vec3<u32>(0u, 10u, 20u)) & vec3<u32>(0x3FFu);
    return (hi << vec3<u32>(10u)) | lo;
}

fn decode_position(data: vec2<u32>) -> vec3<f32> {
    return vec3<f32>(deinterleave_u20x3(data)) * VERTEX_SCALE + VERTEX_OFFSET;
}

fn decode_texcoord(tex: vec2<u32>) -> vec2<f32> {
    return vec2<f32>(tex & vec2<u32>(0x7FFFu)) / TEXTURE_MAX_COORD;
}

fn decode_texcoord_bias(tex: vec2<u32>) -> vec2<f32> {
    return select(
        vec2<f32>(-1.0),
        vec2<f32>(1.0),
        (tex & vec2<u32>(0x8000u)) != vec2<u32>(0u),
    );
}

fn decode_lightmap_uv(light: vec2<u32>) -> vec2<f32> {
    return vec2<f32>(light) / vec2<f32>(256.0);
}

fn octahedral_decode(p: vec2<f32>) -> vec3<f32> {
    let f = p * 2.0 - 1.0;
    var n = vec3<f32>(f.x, f.y, 1.0 - abs(f.x) - abs(f.y));
    let t = clamp(-n.z, 0.0, 1.0);
    n.x = select(n.x + t, n.x - t, n.x >= 0.0);
    n.y = select(n.y + t, n.y - t, n.y >= 0.0);
    return normalize(n);
}

fn octahedral_tangent_decode(p_oct: vec2<f32>) -> vec4<f32> {
    var y = p_oct.y * 2.0 - 1.0;
    let r_sign = select(-1.0, 1.0, y >= 0.0);
    y = abs(y);
    let res = octahedral_decode(vec2<f32>(p_oct.x, y));
    return vec4<f32>(res, r_sign);
}
