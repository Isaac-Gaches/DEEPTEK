struct VertexInput {
    @location(0) position: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct Sky {
    time_of_day: f32,
    aspect_ratio: f32,
    cloud_elapsed_seconds: f32,
    _padding0: f32,
    top_colour: vec3<f32>,
    bottom_colour: vec3<f32>,
    cloud_main: vec3<f32>,
    cloud_edge: vec3<f32>,
};

@group(0) @binding(0)
var<uniform> sky: Sky;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(input.position * 2.0, 0.99, 1.0);
    output.uv = input.position * 2.0;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let sky_colour = mix(sky.bottom_colour, sky.top_colour, (input.uv.y + 1.0) * 0.5);
    let vignette_position = input.uv - vec2<f32>(0.0, 1.0);
    let distance_squared = dot(vignette_position, vignette_position);

    // Clouds occupy only the upper sky. Avoid evaluating procedural noise for
    // pixels outside their visible vignette.
    if distance_squared >= 1.44 {
        return vec4<f32>(sky_colour, 1.0);
    }

    // Keep the procedural coordinate system square as the window changes shape.
    let position = input.uv * vec2<f32>(sky.aspect_ratio, 1.0) - vec2<f32>(0.0, 1.0);
    // Keep cloud motion independent from the normalized day clock so crossing
    // midnight does not jump the procedural noise coordinates back to zero.
    let time = sky.cloud_elapsed_seconds * 0.01;
    let warp = noise(position * 1.15 + vec2<f32>(time * 0.35, 4.2)) - 0.5;
    let moving_position = position * 2.0
        + vec2<f32>(time, 0.0)
        + vec2<f32>(warp * 0.9, warp * 0.35);
    var cloud = noise(moving_position) * 0.55;
    cloud += noise(moving_position * 2.03 + vec2<f32>(17.0, 11.0)) * 0.30;
    cloud += noise(moving_position * 4.11 + vec2<f32>(31.0, 7.0)) * 0.15;
    cloud = smoothstep(0.43, 0.68, cloud);
    let density = cloud * cloud;
    let cloud_colour = mix(sky.cloud_edge, sky.cloud_main, density);
    let vignette = 1.0 - smoothstep(0.64, 1.44, distance_squared);
    let alpha = cloud * vignette * 0.4;
    return vec4<f32>(sky_colour + cloud_colour * alpha, 1.0);
}

fn hash(position: vec2<f32>) -> vec2<f32> {
    let p3 = fract(vec3<f32>(position.xyx) * 0.1031);
    let q = p3 + dot(p3, p3.yzx + 33.33);
    return fract((q.xx + q.yz) * q.zy) * 2.0 - 1.0;
}

fn noise(position: vec2<f32>) -> f32 {
    let cell = floor(position);
    let local = fract(position);
    let smooth_local = local * local * (3.0 - 2.0 * local);
    let a = dot(hash(cell), local);
    let b = dot(hash(cell + vec2<f32>(1.0, 0.0)), local - vec2<f32>(1.0, 0.0));
    let c = dot(hash(cell + vec2<f32>(0.0, 1.0)), local - vec2<f32>(0.0, 1.0));
    let d = dot(hash(cell + vec2<f32>(1.0, 1.0)), local - vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, smooth_local.x), mix(c, d, smooth_local.x), smooth_local.y) * 0.5 + 0.5;
}
