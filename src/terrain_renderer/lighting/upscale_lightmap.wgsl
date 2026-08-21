@group(0) @binding(0)
var input_texture: texture_2d<f32>;

@group(0) @binding(1)
var output_texture: texture_storage_2d<rgba8unorm, write>;

@compute @workgroup_size(16, 16)
fn upscale_lightmap(@builtin(global_invocation_id) gid: vec3<u32>) {
    let output_size = textureDimensions(output_texture);
    if gid.x >= output_size.x || gid.y >= output_size.y {
        return;
    }
    let current = textureLoad(input_texture, vec2<i32>(gid.xy / 2u), 0);
    textureStore(output_texture, vec2<i32>(gid.xy), current);
}
