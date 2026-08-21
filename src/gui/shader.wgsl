struct VertexInput {
    @location(0) position: vec2<f32>,
};

struct InstanceInput {
    @location(1) position: vec2<f32>,
    @location(2) size: vec2<f32>,
    @location(3) depth: f32,
    @location(4) frame: u32,
    @location(5) tint: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) tint: vec4<f32>,
};

struct GuiUniform {
    viewport: vec2<f32>,
    _padding: vec2<f32>,
};

struct AtlasFrame {
    min_uv: vec2<f32>,
    max_uv: vec2<f32>,
};

@group(0) @binding(0)
var gui_texture: texture_2d<f32>;

@group(0) @binding(1)
var gui_sampler: sampler;

@group(0) @binding(2)
var<storage, read> atlas: array<AtlasFrame>;

@group(0) @binding(3)
var<uniform> gui: GuiUniform;

@vertex
fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    let pixel = instance.position + vertex.position * instance.size;
    let frame = atlas[instance.frame];
    // Pixel-space GUI coordinates and texture V both increase downwards.
    let quad_uv = vertex.position + vec2<f32>(0.5);

    var output: VertexOutput;
    output.clip_position = vec4<f32>(
        pixel.x / gui.viewport.x * 2.0 - 1.0,
        1.0 - pixel.y / gui.viewport.y * 2.0,
        instance.depth,
        1.0,
    );
    output.uv = frame.min_uv + quad_uv * (frame.max_uv - frame.min_uv);
    output.tint = instance.tint;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let colour = textureSample(gui_texture, gui_sampler, input.uv) * input.tint;
    if colour.a < 0.01 {
        discard;
    }
    return colour;
}
