@group(0) @binding(0)
var tiles: texture_2d<u32>;

@group(0) @binding(1)
var output_texture: texture_storage_2d<rgba8unorm, write>;

@compute @workgroup_size(16, 16)
fn set_occlusion_map(@builtin(global_invocation_id) gid: vec3<u32>) {
    let size = textureDimensions(tiles);
    if gid.x >= size.x || gid.y >= size.y {
        return;
    }
    let tile_y = size.y - 1u - gid.y;
    let tile = textureLoad(tiles, vec2<u32>(gid.x, tile_y), 0).r;
    let value = select(0.8, 0.0, tile == 1u);
    textureStore(output_texture, gid.xy, vec4<f32>(value, value, value, 1.0));
}
