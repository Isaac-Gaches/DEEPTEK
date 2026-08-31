struct Camera {
    position: vec2<f32>,
    scale: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(0) @binding(1)
var beam_texture: texture_2d<f32>;

@group(0) @binding(2)
var beam_sampler: sampler;

struct VertexInput {
    @location(0) position: vec2<f32>,
};

struct InstanceInput {
    @location(1) position: vec3<f32>,
    @location(2) size: vec2<f32>,
    @location(3) direction: vec2<f32>,
    @location(4) beam_kind: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    let perpendicular = vec2<f32>(-instance.direction.y, instance.direction.x);
    let world_position = instance.position.xy
        + perpendicular * vertex.position.x * instance.size.x
        + instance.direction * vertex.position.y * instance.size.y;
    var output: VertexOutput;
    output.clip_position = vec4<f32>(
        (world_position - camera.position) * camera.scale,
        instance.position.z,
        1.0,
    );
    let quad_uv = vertex.position * vec2<f32>(1.0, -1.0) + vec2<f32>(0.5);
    let cropped_x = 0.28 + quad_uv.x * 0.44;
    output.uv = vec2<f32>(
        (f32(instance.beam_kind) + cropped_x) * 0.5,
        quad_uv.y,
    );
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let colour = textureSample(beam_texture, beam_sampler, input.uv);
    if colour.a < 0.01 {
        discard;
    }
    return colour;
}
