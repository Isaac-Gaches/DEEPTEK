struct Camera {
    position: vec2<f32>,
    scale: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(0) @binding(1)
var decoration_texture: texture_2d<f32>;

@group(0) @binding(2)
var decoration_sampler: sampler;

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

struct VertexInput {
    @location(0) position: vec2<f32>,
};

struct InstanceInput {
    @location(1) position: vec3<f32>,
    @location(2) frame: u32,
    @location(3) visual_kind: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) light_uv: vec2<f32>,
    @location(2) local_uv: vec2<f32>,
    @location(3) @interpolate(flat) visual_kind: u32,
};

@vertex
fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    var output: VertexOutput;
    let world_position = instance.position.xy + vertex.position;
    output.clip_position = vec4<f32>(
        (world_position - camera.position) * camera.scale,
        instance.position.z,
        1.0,
    );

    var frame_min = vec2<f32>(f32(instance.frame) * 0.2, 0.0);
    if instance.frame >= 3u {
        frame_min = vec2<f32>(f32(instance.frame - 1u) * 0.2, 0.5);
    }
    let quad_uv = vertex.position * vec2<f32>(1.0, -1.0) + vec2<f32>(0.5);
    output.uv = frame_min + quad_uv * vec2<f32>(0.2, 0.5);
    output.local_uv = quad_uv;
    output.visual_kind = instance.visual_kind;

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
    if input.visual_kind == 1u || input.visual_kind == 2u {
        let centre = 0.5 + sin(input.local_uv.y * 12.56637) * 0.035;
        var half_width = 0.075;
        if input.visual_kind == 2u && input.local_uv.y > 0.72 {
            half_width = 0.14;
        }
        if abs(input.local_uv.x - centre) > half_width {
            discard;
        }
        let braid = step(0.5, fract(input.local_uv.y * 5.0 + input.local_uv.x * 2.0));
        let rope_colour = mix(vec3<f32>(0.34, 0.16, 0.055), vec3<f32>(0.68, 0.39, 0.13), braid);
        let light = textureSample(light_texture, light_sampler, input.light_uv);
        return vec4<f32>(rope_colour, 1.0) * light;
    }
    if input.visual_kind == 3u || input.visual_kind == 4u {
        var half_width = 0.095;
        if input.visual_kind == 4u && input.local_uv.y > 0.76 {
            half_width = 0.17;
        }
        let distance = abs(input.local_uv.x - 0.5);
        if distance > half_width {
            discard;
        }
        if distance < 0.022 {
            return vec4<f32>(0.0, 1.0, 1.0, 1.0);
        }
        let stripe = step(0.5, fract(input.local_uv.y * 8.0));
        let cable_colour = mix(vec3<f32>(0.08, 0.12, 0.14), vec3<f32>(0.24, 0.31, 0.33), stripe);
        return vec4<f32>(cable_colour, 1.0) *
            textureSample(light_texture, light_sampler, input.light_uv);
    }
    let colour = textureSample(decoration_texture, decoration_sampler, input.uv);
    if colour.a < 0.5 {
        discard;
    }
    if is_emissive_key(colour.rgb) {
        return colour;
    }
    return colour * textureSample(light_texture, light_sampler, input.light_uv);
}
