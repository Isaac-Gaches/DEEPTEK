struct Camera {
    position: vec2<f32>,
    scale: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(0) @binding(1)
var sprite_texture: texture_2d<f32>;

@group(0) @binding(2)
var sprite_sampler: sampler;

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

struct AtlasFrame {
    min_uv: vec2<f32>,
    max_uv: vec2<f32>,
};

@group(0) @binding(6)
var<storage, read> atlas: array<AtlasFrame>;

struct VertexInput {
    @location(0) position: vec2<f32>,
};

struct InstanceInput {
    @location(1) position: vec3<f32>,
    @location(2) rotation: f32,
    @location(3) scale: vec2<f32>,
    @location(4) frame: u32,
    @location(5) tint: vec4<f32>,
    @location(6) emissive: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) light_uv: vec2<f32>,
    @location(2) tint: vec4<f32>,
    @location(3) emissive: f32,
};

@vertex
fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    let local = vertex.position * instance.scale;
    let cosine = cos(instance.rotation);
    let sine = sin(instance.rotation);
    let rotated = vec2<f32>(
        local.x * cosine - local.y * sine,
        local.x * sine + local.y * cosine,
    );
    let world_position = instance.position.xy + rotated;
    let frame = atlas[instance.frame];
    let quad_uv = vertex.position * vec2<f32>(1.0, -1.0) + vec2<f32>(0.5);
    let logical_position = vec2<f32>(world_position.x, -world_position.y);
    let lightmap_size = vec2<f32>(
        light_meta.horizontal_render_distance * 2.0 + light_meta.chunk_size,
        light_meta.vertical_render_distance * 2.0 + light_meta.chunk_size,
    );

    var output: VertexOutput;
    output.clip_position = vec4<f32>(
        (world_position - camera.position) * camera.scale,
        instance.position.z,
        1.0,
    );
    output.uv = frame.min_uv + quad_uv * (frame.max_uv - frame.min_uv);
    output.light_uv = vec2<f32>(
        (logical_position.x + 0.5 + light_meta.horizontal_render_distance - light_meta.anchor.x)
            / lightmap_size.x,
        1.0 - (logical_position.y + 0.5 + light_meta.vertical_render_distance
            - light_meta.anchor.y) / lightmap_size.y,
    );
    output.tint = instance.tint;
    output.emissive = instance.emissive;
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
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(sprite_texture, sprite_sampler, input.uv);
    let colour = texel * input.tint;
    // A few source atlases contain near-zero alpha cleanup pixels. Discarding
    // them prevents those pixels writing depth and punching holes in layered
    // sprites such as the player's body behind its arm.
    if colour.a < 0.02 {
        discard;
    }
    if is_emissive_key(texel.rgb) {
        return vec4<f32>(texel.rgb, colour.a);
    }
    if is_emissive_key(colour.rgb) {
        return colour;
    }
    // Fully emissive effects may use an atlas shape as an alpha mask while
    // supplying one of the six colour keys through their instance tint.
    if input.emissive >= 1.0 - 1e-5 && is_emissive_key(input.tint.rgb) {
        return vec4<f32>(input.tint.rgb, colour.a);
    }
    let terrain_light = textureSample(light_texture, light_sampler, input.light_uv).rgb;
    let illumination = mix(terrain_light, vec3<f32>(1.0), clamp(input.emissive, 0.0, 1.0));
    return vec4<f32>(colour.rgb * illumination, colour.a);
}
