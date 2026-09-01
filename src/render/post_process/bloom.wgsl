struct VertexInput {
    @location(0) position: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
}

const BLOOM_INTENSITY: f32 = 0.75;

@group(0) @binding(0) var source_texture: texture_2d<f32>;
@group(0) @binding(1) var bloom_texture: texture_2d<f32>;
@group(0) @binding(2) var linear_sampler: sampler;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(input.position * 2.0, 0.0, 1.0);
    return output;
}

// On an 8-bit scene target, this half-step accepts only channel endpoints.
// Excluding black and white leaves the three additive primaries and their
// three secondary combinations.
fn is_bloom_colour(colour: vec3<f32>) -> bool {
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
fn fs_extract(input: VertexOutput) -> @location(0) vec4<f32> {
    let source_size = textureDimensions(source_texture);
    let output_pixel = vec2<u32>(input.position.xy);
    let source_origin = output_pixel * 4u;
    var colour_sum = vec3<f32>(0.0);

    for (var y = 0u; y < 4u; y++) {
        for (var x = 0u; x < 4u; x++) {
            let source_pixel = source_origin + vec2<u32>(x, y);
            if all(source_pixel < source_size) {
                let colour = textureLoad(
                    source_texture,
                    vec2<i32>(source_pixel),
                    0,
                ).rgb;
                if is_bloom_colour(colour) {
                    colour_sum += colour;
                }
            }
        }
    }

    return vec4<f32>(colour_sum / 16.0, 1.0);
}

fn gaussian_blur(position: vec2<f32>, direction: vec2<f32>) -> vec4<f32> {
    let texture_size = vec2<f32>(textureDimensions(source_texture));
    let uv = position / texture_size;
    let texel = direction / texture_size;
    var colour = textureSampleLevel(source_texture, linear_sampler, uv, 0.0) * 0.2270270270;
    colour += textureSampleLevel(
        source_texture,
        linear_sampler,
        uv + texel * 1.3846153846,
        0.0,
    ) * 0.3162162162;
    colour += textureSampleLevel(
        source_texture,
        linear_sampler,
        uv - texel * 1.3846153846,
        0.0,
    ) * 0.3162162162;
    colour += textureSampleLevel(
        source_texture,
        linear_sampler,
        uv + texel * 3.2307692308,
        0.0,
    ) * 0.0702702703;
    colour += textureSampleLevel(
        source_texture,
        linear_sampler,
        uv - texel * 3.2307692308,
        0.0,
    ) * 0.0702702703;
    return colour;
}

@fragment
fn fs_blur_horizontal(input: VertexOutput) -> @location(0) vec4<f32> {
    return gaussian_blur(input.position.xy, vec2<f32>(1.0, 0.0));
}

@fragment
fn fs_blur_vertical(input: VertexOutput) -> @location(0) vec4<f32> {
    return gaussian_blur(input.position.xy, vec2<f32>(0.0, 1.0));
}

@fragment
fn fs_composite(input: VertexOutput) -> @location(0) vec4<f32> {
    let scene_pixel = vec2<i32>(input.position.xy);
    let scene = textureLoad(source_texture, scene_pixel, 0);
    let scene_size = vec2<f32>(textureDimensions(source_texture));
    let uv = input.position.xy / scene_size;
    let glow = textureSampleLevel(bloom_texture, linear_sampler, uv, 0.0);
    return vec4<f32>(scene.rgb + glow.rgb * BLOOM_INTENSITY, scene.a);
}
