struct Camera {
    position: vec2<f32>,
    scale: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(0) @binding(1)
var terrain_texture: texture_2d<f32>;

@group(0) @binding(2)
var terrain_sampler: sampler;

@group(0) @binding(3)
var light_texture: texture_2d<f32>;

@group(0) @binding(4)
var light_sampler: sampler;

struct LightMapMeta {
    anchor: vec2<f32>,
    vertical_render_distance: f32,
    horizontal_render_distance: f32,
    chunk_size: f32,
    _padding: f32,
};

@group(0) @binding(5)
var<uniform> light_meta: LightMapMeta;

@group(0) @binding(6)
var occlusion_texture: texture_2d<f32>;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) light_uv: vec2<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.clip_position = vec4<f32>(
        (input.position.xy - camera.position) * camera.scale,
        input.position.z,
        1.0,
    );
    output.uv = input.uv;
    let logical_position = vec2<f32>(input.position.x, -input.position.y);
    let lightmap_size = vec2<f32>(
        light_meta.horizontal_render_distance * 2.0 + light_meta.chunk_size,
        light_meta.vertical_render_distance * 2.0 + light_meta.chunk_size,
    );
    output.light_uv = vec2<f32>(
        (logical_position.x + 0.5 + light_meta.horizontal_render_distance - light_meta.anchor.x)
            / lightmap_size.x,
        1.0 - (logical_position.y + 0.5 + light_meta.vertical_render_distance
            - light_meta.anchor.y) / lightmap_size.y,
    );
    return output;
}

fn is_emissive_key(colour: vec3<f32>) -> bool {
    let half_unorm_step = 0.5 / 255.0;
    let endpoint = (colour <= vec3<f32>(half_unorm_step)) |
        (colour >= vec3<f32>(1.0 - half_unorm_step));
    if !all(endpoint) {
        return false;
    }
    let high = colour >= vec3<f32>(1.0 - half_unorm_step);
    return any(high) && !all(high);
}

@fragment
fn fs_foreground(input: VertexOutput) -> @location(0) vec4<f32> {
    var colour = textureSample(terrain_texture, terrain_sampler, input.uv);
    if colour.a < 0.5 {
        discard;
    }
    if is_emissive_key(colour.rgb) {
        return colour;
    }
    let light = textureSample(light_texture, light_sampler, input.light_uv);
    return colour * light;
}

@fragment
fn fs_background(input: VertexOutput) -> @location(0) vec4<f32> {
    let colour = textureSample(terrain_texture, terrain_sampler, input.uv);
    if colour.a < 0.5 {
        discard;
    }
    if is_emissive_key(colour.rgb) {
        return colour;
    }
    let light = textureSample(light_texture, light_sampler, input.light_uv);
    let occlusion = textureSample(occlusion_texture, light_sampler, input.light_uv).r;
    return colour * light * 0.2 * max(occlusion, 0.5);
}
