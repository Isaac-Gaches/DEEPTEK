@group(0) @binding(0)
var input_texture: texture_2d<f32>;

@group(0) @binding(1)
var output_texture: texture_storage_2d<rgba8unorm, write>;

@group(0) @binding(2)
var tiles: texture_2d<u32>;

@group(0) @binding(3)
var<uniform> sky_light: vec3<f32>;

@compute @workgroup_size(16, 16)
fn set_sky_light(@builtin(global_invocation_id) gid: vec3<u32>) {
    let size = textureDimensions(input_texture);
    if gid.x >= size.x || gid.y >= size.y {
        return;
    }
    let tile_y = size.y - 1u - gid.y;
    let tile = textureLoad(tiles, vec2<u32>(gid.x, tile_y), 0).r;
    let current = textureLoad(input_texture, vec2<i32>(gid.xy), 0);
    let output = select(current, max(vec4<f32>(sky_light, 1.0), current), tile == 0u);
    textureStore(output_texture, vec2<i32>(gid.xy), output);
}
