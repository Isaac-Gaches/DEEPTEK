struct Camera {
    position: vec2<f32>,
    scale: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(0) @binding(1)
var furniture_texture: texture_2d<f32>;

@group(0) @binding(2)
var furniture_sampler: sampler;

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
    @location(2) size: vec2<f32>,
    @location(3) uv_rect: vec4<f32>,
    @location(4) visual_kind: u32,
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
    let world_position = instance.position.xy + vertex.position * instance.size;
    output.clip_position = vec4<f32>(
        (world_position - camera.position) * camera.scale,
        instance.position.z,
        1.0,
    );
    let quad_uv = vertex.position * vec2<f32>(1.0, -1.0) + vec2<f32>(0.5);
    output.uv = instance.uv_rect.xy + quad_uv * instance.uv_rect.zw;
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
    if input.visual_kind == 1u {
        let delta = input.local_uv - vec2<f32>(0.5);
        let radius = length(delta);
        if radius > 0.39 {
            discard;
        }
        if radius < 0.105 {
            return vec4<f32>(0.0, 1.0, 1.0, 1.0);
        }
        let ring = select(vec3<f32>(0.18, 0.22, 0.24), vec3<f32>(0.48, 0.54, 0.55), radius < 0.29);
        return vec4<f32>(ring, 1.0) * textureSample(light_texture, light_sampler, input.light_uv);
    }
    if input.visual_kind == 2u || input.visual_kind == 3u {
        let border = input.local_uv.x < 0.08 || input.local_uv.x > 0.92 ||
            input.local_uv.y < 0.08 || input.local_uv.y > 0.92;
        let door = input.local_uv.x > 0.30 && input.local_uv.x < 0.70 && input.local_uv.y > 0.25;
        let bracket = (input.visual_kind == 2u && input.local_uv.x < 0.13) ||
            (input.visual_kind == 3u && input.local_uv.x > 0.87);
        if input.local_uv.y < 0.19 && input.local_uv.x > 0.72 && input.local_uv.x < 0.82 {
            return vec4<f32>(1.0, 1.0, 0.0, 1.0);
        }
        var lift_colour = vec3<f32>(0.36, 0.41, 0.43);
        if border || bracket {
            lift_colour = vec3<f32>(0.10, 0.13, 0.15);
        } else if door {
            lift_colour = vec3<f32>(0.20, 0.25, 0.27);
        }
        return vec4<f32>(lift_colour, 1.0) *
            textureSample(light_texture, light_sampler, input.light_uv);
    }
    if input.visual_kind == 4u || input.visual_kind == 5u {
        let border = input.local_uv.x < 0.07 || input.local_uv.x > 0.93 ||
            input.local_uv.y < 0.08 || input.local_uv.y > 0.92;
        let cable_bracket = (input.visual_kind == 4u && input.local_uv.x < 0.13) ||
            (input.visual_kind == 5u && input.local_uv.x > 0.87);
        let display = input.local_uv.x > 0.24 && input.local_uv.x < 0.76 &&
            input.local_uv.y > 0.18 && input.local_uv.y < 0.40;
        let divider = input.local_uv.y > 0.53 && input.local_uv.y < 0.59;
        if display && input.local_uv.x > 0.34 && input.local_uv.x < 0.66 {
            return vec4<f32>(0.0, 1.0, 1.0, 1.0);
        }
        var station_colour = vec3<f32>(0.30, 0.36, 0.38);
        if border || cable_bracket || divider {
            station_colour = vec3<f32>(0.08, 0.12, 0.14);
        } else if display {
            station_colour = vec3<f32>(0.04, 0.10, 0.12);
        } else if input.local_uv.y > 0.62 {
            station_colour = vec3<f32>(0.20, 0.25, 0.27);
        }
        return vec4<f32>(station_colour, 1.0) *
            textureSample(light_texture, light_sampler, input.light_uv);
    }
    if input.visual_kind == 6u {
        let delta = input.local_uv - vec2<f32>(0.5);
        let radius = length(delta);
        let horizontal = abs(delta.y) < 0.055 && abs(delta.x) < 0.28;
        let vertical = abs(delta.x) < 0.055 && abs(delta.y) < 0.28;
        if radius > 0.40 || (radius < 0.24 && !horizontal && !vertical) {
            discard;
        }
        var connector_colour = vec3<f32>(0.13, 0.17, 0.19);
        if radius < 0.33 && radius > 0.27 {
            connector_colour = vec3<f32>(0.44, 0.52, 0.54);
        }
        if horizontal || vertical {
            connector_colour = vec3<f32>(0.0, 1.0, 1.0);
        }
        return vec4<f32>(connector_colour, 1.0) *
            textureSample(light_texture, light_sampler, input.light_uv);
    }
    let colour = textureSample(furniture_texture, furniture_sampler, input.uv);
    if colour.a < 0.5 {
        discard;
    }
    if is_emissive_key(colour.rgb) {
        return colour;
    }
    return colour * textureSample(light_texture, light_sampler, input.light_uv);
}
