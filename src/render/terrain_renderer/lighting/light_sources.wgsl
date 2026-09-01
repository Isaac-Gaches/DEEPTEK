struct LightSource {
    position: vec2<f32>,
    _padding0: vec2<f32>,
    colour: vec4<f32>,
};

struct LightSourceMeta {
    anchor: vec2<f32>,
    light_count: u32,
    _padding: u32,
    midpoint: vec2<i32>,
};

@group(0) @binding(0)
var output_texture: texture_storage_2d<rgba8unorm, write>;

@group(0) @binding(1)
var<storage, read> lights: array<LightSource>;

@group(0) @binding(2)
var<uniform> light_meta: LightSourceMeta;

@compute @workgroup_size(64, 1, 1)
fn set_light_sources(@builtin(global_invocation_id) gid: vec3<u32>) {
    if gid.x >= light_meta.light_count {
        return;
    }
    let light = lights[gid.x];
    let size = textureDimensions(output_texture);
    var pixel_x = i32(light.position.x - light_meta.anchor.x) + light_meta.midpoint.x;
    var pixel_y = i32(light.position.y - light_meta.anchor.y) + light_meta.midpoint.y;
    pixel_y = i32(size.y) - 1 - pixel_y;
    if pixel_x < 0 || pixel_y < 0 || pixel_x >= i32(size.x) || pixel_y >= i32(size.y) {
        return;
    }
    textureStore(output_texture, vec2<i32>(pixel_x, pixel_y), light.colour);
}
