@group(0) @binding(0)
var input_texture: texture_2d<f32>;

@group(0) @binding(1)
var output_texture: texture_storage_2d<rgba8unorm, write>;

@group(0) @binding(2)
var tiles: texture_2d<u32>;

fn load_clamped(position: vec2<i32>, size: vec2<i32>) -> vec3<f32> {
    let clamped = clamp(position, vec2<i32>(0), size - 1);
    return textureLoad(input_texture, clamped, 0).rgb;
}

@compute @workgroup_size(16, 16)
fn diffuse_light(@builtin(global_invocation_id) gid: vec3<u32>) {
    let size_u = textureDimensions(input_texture);
    if gid.x >= size_u.x || gid.y >= size_u.y {
        return;
    }
    let position = vec2<i32>(gid.xy);
    let tile_y = size_u.y - 1u - gid.y;
    let tile = textureLoad(tiles, vec2<u32>(gid.x, tile_y), 0).r;
    let current = textureLoad(input_texture, position, 0).rgb;
    if tile == 1u {
        textureStore(output_texture, position, vec4<f32>(current, 1.0));
        return;
    }
    let size = vec2<i32>(size_u);
    let decay = 0.85;
    let left = load_clamped(position + vec2<i32>(-1, 0), size) * decay;
    let right = load_clamped(position + vec2<i32>(1, 0), size) * decay;
    let up = load_clamped(position + vec2<i32>(0, 1), size) * decay;
    let down = load_clamped(position + vec2<i32>(0, -1), size) * decay;
    let propagated = max(max(left, right), max(up, down));
    textureStore(output_texture, position, vec4<f32>(max(current, propagated), 1.0));
}
