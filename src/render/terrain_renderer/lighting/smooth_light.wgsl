@group(0) @binding(0)
var input_texture: texture_2d<f32>;

@group(0) @binding(1)
var output_texture: texture_storage_2d<rgba8unorm, write>;

@compute @workgroup_size(16, 16)
fn smooth_light(@builtin(global_invocation_id) gid: vec3<u32>) {
    let size = textureDimensions(input_texture);
    if gid.x >= size.x || gid.y >= size.y {
        return;
    }
    let pixel = vec2<i32>(gid.xy);
    var sum = vec4<f32>(0.0);
    for (var offset_y = -1; offset_y <= 1; offset_y += 1) {
        for (var offset_x = -1; offset_x <= 1; offset_x += 1) {
            let sample_position = clamp(
                pixel + vec2<i32>(offset_x, offset_y),
                vec2<i32>(0),
                vec2<i32>(size) - 1,
            );
            sum += textureLoad(input_texture, sample_position, 0);
        }
    }
    let current = textureLoad(input_texture, pixel, 0);
    let blurred = sum / 9.0;
    textureStore(output_texture, pixel, vec4<f32>(max(blurred.rgb, current.rgb), 1.0));
}
