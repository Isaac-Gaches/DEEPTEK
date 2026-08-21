struct Camera {
    position: vec2<f32>,
    scale: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(0) @binding(1)
var light_texture: texture_2d<f32>;

@group(0) @binding(2)
var light_sampler: sampler;

struct LightMapMeta {
    anchor: vec2<f32>,
    vertical_render_distance: f32,
    horizontal_render_distance: f32,
    chunk_size: f32,
    _padding: f32,
};

@group(0) @binding(3)
var<uniform> light_meta: LightMapMeta;

struct VertexInput {
    @location(0) position: vec2<f32>,
};

struct InstanceInput {
    @location(1) position: vec3<f32>,
    @location(2) size: vec2<f32>,
    @location(3) direction: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) light_uv: vec2<f32>,
};

@vertex
fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    let normal = vec2<f32>(-instance.direction.y, instance.direction.x);
    let world_position = instance.position.xy
        + instance.direction * vertex.position.x * instance.size.x
        + normal * vertex.position.y * instance.size.y;
    var output: VertexOutput;
    output.clip_position = vec4<f32>(
        (world_position - camera.position) * camera.scale,
        instance.position.z,
        1.0,
    );
    let logical_position = vec2<f32>(world_position.x, -world_position.y);
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

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let cable_colour = vec4<f32>(0.22, 0.29, 0.31, 1.0);
    return cable_colour * textureSample(light_texture, light_sampler, input.light_uv);
}
