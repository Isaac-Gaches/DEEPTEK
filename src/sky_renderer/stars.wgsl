struct VertexInput {
    @location(0) position: vec2<f32>,
};

struct StarInput {
    @location(1) position: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) brightness: f32,
    @location(1) uv: vec2<f32>,
};

struct Time {
    time_of_day: f32,
    aspect_ratio: f32,
    _padding: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> time: Time;

fn hash(position: vec2<f32>) -> f32 {
    return fract(sin(dot(position, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn star_visibility(value: f32) -> f32 {
    if value >= 0.8 {
        return smoothstep(0.8, 0.9, value);
    }
    if value <= 0.1 {
        return 1.0;
    }
    if value <= 0.2 {
        return 1.0 - smoothstep(0.1, 0.2, value);
    }
    return 0.0;
}

@vertex
fn vs_main(vertex: VertexInput, star: StarInput) -> VertexOutput {
    var output: VertexOutput;
    let pixel_correct_scale = vec2<f32>(0.003 / time.aspect_ratio, 0.003);
    output.position = vec4<f32>(vertex.position * pixel_correct_scale + star.position, 0.995, 1.0);
    output.uv = vertex.position * 2.0;
    let phase = hash(star.position) * 6.28318;
    let twinkle = 0.6 + 0.4 * sin(time.time_of_day * 6.28318 * 500.0 + phase);
    output.brightness = twinkle * star_visibility(time.time_of_day)
        * (star.position.y + 1.0) * 0.5;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let alpha = max(0.0, 1.0 - length(input.uv)) * input.brightness;
    return vec4<f32>(1.0, 1.0, 1.0, alpha);
}
