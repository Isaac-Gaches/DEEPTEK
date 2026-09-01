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

fn segment_distance(point: vec2<f32>, start: vec2<f32>, end: vec2<f32>) -> f32 {
    let segment = end - start;
    let progress = clamp(dot(point - start, segment) / dot(segment, segment), 0.0, 1.0);
    return length(point - (start + segment * progress));
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
    if input.visual_kind == 7u {
        let border = input.local_uv.x < 0.045 || input.local_uv.x > 0.955 ||
            input.local_uv.y < 0.07 || input.local_uv.y > 0.93;
        let hopper_left = input.local_uv.x > 0.10 && input.local_uv.x < 0.30 &&
            input.local_uv.y > 0.12 && input.local_uv.y < 0.43;
        let hopper_right = input.local_uv.x > 0.34 && input.local_uv.x < 0.54 &&
            input.local_uv.y > 0.12 && input.local_uv.y < 0.43;
        let output_bay = input.local_uv.x > 0.68 && input.local_uv.x < 0.90 &&
            input.local_uv.y > 0.50 && input.local_uv.y < 0.82;
        let display = input.local_uv.x > 0.68 && input.local_uv.x < 0.90 &&
            input.local_uv.y > 0.16 && input.local_uv.y < 0.34;
        let drum_delta = input.local_uv - vec2<f32>(0.42, 0.69);
        let drum = length(drum_delta / vec2<f32>(1.0, 1.55)) < 0.17;
        var machine_colour = vec3<f32>(0.27, 0.33, 0.35);
        if border {
            machine_colour = vec3<f32>(0.07, 0.10, 0.12);
        } else if hopper_left || hopper_right || output_bay {
            machine_colour = vec3<f32>(0.12, 0.17, 0.19);
        } else if drum {
            machine_colour = vec3<f32>(0.45, 0.51, 0.52);
        } else if display {
            machine_colour = vec3<f32>(0.0, 0.82, 0.90);
        }
        return vec4<f32>(machine_colour, 1.0) *
            textureSample(light_texture, light_sampler, input.light_uv);
    }
    if input.visual_kind == 8u {
        let border = input.local_uv.x < 0.035 || input.local_uv.x > 0.965 ||
            input.local_uv.y < 0.055 || input.local_uv.y > 0.945;
        let left_leg = input.local_uv.x < 0.12 && input.local_uv.y > 0.58;
        let right_leg = input.local_uv.x > 0.88 && input.local_uv.y > 0.58;
        let open_shaft = input.local_uv.x > 0.17 && input.local_uv.x < 0.83 &&
            input.local_uv.y > 0.64;
        let red_lens = input.local_uv.x > 0.20 && input.local_uv.x < 0.80 &&
            input.local_uv.y > 0.48 && input.local_uv.y < 0.57;
        let display = input.local_uv.x > 0.72 && input.local_uv.x < 0.90 &&
            input.local_uv.y > 0.16 && input.local_uv.y < 0.32;
        if open_shaft && !red_lens {
            discard;
        }
        var bore_colour = vec3<f32>(0.25, 0.29, 0.31);
        if border || left_leg || right_leg {
            bore_colour = vec3<f32>(0.06, 0.08, 0.10);
        } else if red_lens {
            bore_colour = vec3<f32>(1.0, 0.02, 0.01);
        } else if display {
            bore_colour = vec3<f32>(0.45, 0.03, 0.02);
        } else if input.local_uv.y < 0.40 {
            bore_colour = vec3<f32>(0.38, 0.42, 0.43);
        }
        return vec4<f32>(bore_colour, 1.0) *
            textureSample(light_texture, light_sampler, input.light_uv);
    }
    if input.visual_kind == 9u {
        let border = input.local_uv.x < 0.07 || input.local_uv.x > 0.93 ||
            input.local_uv.y < 0.06 || input.local_uv.y > 0.94;
        let screen = input.local_uv.x > 0.14 && input.local_uv.x < 0.86 &&
            input.local_uv.y > 0.12 && input.local_uv.y < 0.55;
        let screen_inner = input.local_uv.x > 0.19 && input.local_uv.x < 0.81 &&
            input.local_uv.y > 0.17 && input.local_uv.y < 0.50;
        let keyboard = input.local_uv.x > 0.18 && input.local_uv.x < 0.82 &&
            input.local_uv.y > 0.66 && input.local_uv.y < 0.79;
        let base = input.local_uv.y > 0.84;
        var terminal_colour = vec3<f32>(0.25, 0.31, 0.34);
        if screen_inner {
            terminal_colour = vec3<f32>(0.0, 0.85, 0.92);
        } else if border || base {
            terminal_colour = vec3<f32>(0.06, 0.09, 0.11);
        } else if screen {
            terminal_colour = vec3<f32>(0.03, 0.10, 0.13);
        } else if keyboard {
            terminal_colour = vec3<f32>(0.42, 0.49, 0.51);
        }
        if screen_inner {
            return vec4<f32>(terminal_colour, 1.0);
        }
        return vec4<f32>(terminal_colour, 1.0) *
            textureSample(light_texture, light_sampler, input.light_uv);
    }
    if input.visual_kind >= 10u && input.visual_kind <= 16u {
        let aim = (f32(input.visual_kind - 10u) - 3.0) / 3.0;
        let pivot = vec2<f32>(0.5, 0.38);
        let direction = normalize(vec2<f32>(aim * 0.72, 1.0));
        let barrel_end = pivot + direction * 0.28;
        let barrel = segment_distance(input.local_uv, pivot, barrel_end) < 0.055;
        let lens = length(input.local_uv - barrel_end) < 0.075;
        let body = input.local_uv.x > 0.08 && input.local_uv.x < 0.92 &&
            input.local_uv.y > 0.08 && input.local_uv.y < 0.55;
        let left_leg = input.local_uv.x > 0.06 && input.local_uv.x < 0.18 &&
            input.local_uv.y > 0.48;
        let right_leg = input.local_uv.x > 0.82 && input.local_uv.x < 0.94 &&
            input.local_uv.y > 0.48;
        if !body && !left_leg && !right_leg && !barrel && !lens {
            discard;
        }
        var drill_colour = vec3<f32>(0.25, 0.31, 0.34);
        if left_leg || right_leg {
            drill_colour = vec3<f32>(0.06, 0.09, 0.11);
        } else if barrel {
            drill_colour = vec3<f32>(0.43, 0.51, 0.54);
        } else if lens {
            return vec4<f32>(0.0, 1.0, 1.0, 1.0);
        }
        return vec4<f32>(drill_colour, 1.0) *
            textureSample(light_texture, light_sampler, input.light_uv);
    }
    if input.visual_kind == 17u || input.visual_kind == 18u {
        let facing = select(-1.0, 1.0, input.visual_kind == 17u);
        let pivot = vec2<f32>(0.5, 0.42);
        let barrel_end = pivot + normalize(vec2<f32>(facing, -0.42)) * 0.34;
        let barrel = segment_distance(input.local_uv, pivot, barrel_end) < 0.055;
        let muzzle = length(input.local_uv - barrel_end) < 0.07;
        let housing = length((input.local_uv - pivot) / vec2<f32>(1.0, 0.82)) < 0.19;
        let base = input.local_uv.x > 0.20 && input.local_uv.x < 0.80 &&
            input.local_uv.y > 0.58 && input.local_uv.y < 0.82;
        let feet = input.local_uv.y > 0.80 &&
            ((input.local_uv.x > 0.18 && input.local_uv.x < 0.30) ||
             (input.local_uv.x > 0.70 && input.local_uv.x < 0.82));
        if !barrel && !muzzle && !housing && !base && !feet {
            discard;
        }
        var colour = vec3<f32>(0.25, 0.29, 0.31);
        if barrel {
            colour = vec3<f32>(0.48, 0.52, 0.53);
        } else if muzzle {
            colour = vec3<f32>(1.0, 0.42, 0.04);
        } else if base || feet {
            colour = vec3<f32>(0.07, 0.09, 0.10);
        } else if housing {
            colour = vec3<f32>(0.36, 0.22, 0.12);
        }
        if muzzle {
            return vec4<f32>(colour, 1.0);
        }
        return vec4<f32>(colour, 1.0) *
            textureSample(light_texture, light_sampler, input.light_uv);
    }
    if input.visual_kind == 19u || input.visual_kind == 20u {
        let facing_right = input.visual_kind == 19u;
        let border = input.local_uv.x < 0.08 || input.local_uv.x > 0.92 ||
            input.local_uv.y < 0.08 || input.local_uv.y > 0.92;
        let barrel = input.local_uv.y > 0.43 && input.local_uv.y < 0.57 &&
            select((input.local_uv.x < 0.30), (input.local_uv.x > 0.70), facing_right);
        let sensor = input.local_uv.y > 0.30 && input.local_uv.y < 0.70 &&
            select((input.local_uv.x > 0.66), (input.local_uv.x < 0.34), facing_right);
        var colour = vec3<f32>(0.24, 0.29, 0.31);
        if border {
            colour = vec3<f32>(0.06, 0.08, 0.09);
        } else if barrel {
            colour = vec3<f32>(0.48, 0.53, 0.54);
        } else if sensor {
            return vec4<f32>(1.0, 0.22, 0.03, 1.0);
        }
        return vec4<f32>(colour, 1.0) *
            textureSample(light_texture, light_sampler, input.light_uv);
    }
    if input.visual_kind == 21u {
        let base = input.local_uv.y > 0.78 && input.local_uv.y < 0.94;
        let repeated_x = fract(input.local_uv.x * 4.0);
        let spike_height = 1.0 - abs(repeated_x * 2.0 - 1.0);
        let spike = input.local_uv.y > 0.78 - spike_height * 0.58 &&
            input.local_uv.y <= 0.82;
        if !base && !spike {
            discard;
        }
        let edge = repeated_x < 0.12 || repeated_x > 0.88;
        let colour = select(
            vec3<f32>(0.43, 0.47, 0.48),
            vec3<f32>(0.12, 0.15, 0.16),
            base || edge,
        );
        return vec4<f32>(colour, 1.0) *
            textureSample(light_texture, light_sampler, input.light_uv);
    }
    if input.visual_kind == 22u || input.visual_kind == 23u {
        if input.visual_kind == 23u {
            let jamb = input.local_uv.x < 0.11 || input.local_uv.x > 0.89 ||
                input.local_uv.y < 0.035 || input.local_uv.y > 0.965;
            let folded_door = input.local_uv.x > 0.75 && input.local_uv.x < 0.89 &&
                input.local_uv.y > 0.05 && input.local_uv.y < 0.95;
            if !jamb && !folded_door {
                discard;
            }
            let open_colour = select(
                vec3<f32>(0.42, 0.27, 0.14),
                vec3<f32>(0.075, 0.085, 0.09),
                jamb,
            );
            return vec4<f32>(open_colour, 1.0) *
                textureSample(light_texture, light_sampler, input.light_uv);
        }
        let frame = input.local_uv.x < 0.10 || input.local_uv.x > 0.90 ||
            input.local_uv.y < 0.035 || input.local_uv.y > 0.965;
        let inset = input.local_uv.x > 0.20 && input.local_uv.x < 0.80 &&
            input.local_uv.y > 0.11 && input.local_uv.y < 0.89;
        let panel_line = inset &&
            (abs(input.local_uv.y - 0.37) < 0.018 || abs(input.local_uv.y - 0.65) < 0.018);
        let handle = distance(input.local_uv, vec2<f32>(0.72, 0.53)) < 0.045;
        var colour = vec3<f32>(0.31, 0.20, 0.12);
        if frame || panel_line {
            colour = vec3<f32>(0.075, 0.085, 0.09);
        } else if inset {
            colour = vec3<f32>(0.43, 0.28, 0.15);
        }
        if handle {
            return vec4<f32>(0.90, 0.68, 0.18, 1.0);
        }
        return vec4<f32>(colour, 1.0) *
            textureSample(light_texture, light_sampler, input.light_uv);
    }
    if input.visual_kind == 24u {
        let frame = input.local_uv.y > 0.76 && input.local_uv.y < 0.90;
        let mattress = input.local_uv.x > 0.05 && input.local_uv.x < 0.95 &&
            input.local_uv.y > 0.26 && input.local_uv.y <= 0.76;
        let pillow = input.local_uv.x > 0.08 && input.local_uv.x < 0.30 &&
            input.local_uv.y > 0.31 && input.local_uv.y < 0.63;
        let leg = (input.local_uv.x > 0.08 && input.local_uv.x < 0.16 ||
            input.local_uv.x > 0.84 && input.local_uv.x < 0.92) &&
            input.local_uv.y >= 0.90;
        if !frame && !mattress && !leg {
            discard;
        }
        var colour = vec3<f32>(0.19, 0.25, 0.28);
        if mattress {
            colour = vec3<f32>(0.48, 0.12, 0.10);
        }
        if pillow {
            colour = vec3<f32>(0.77, 0.78, 0.72);
        }
        return vec4<f32>(colour, 1.0) *
            textureSample(light_texture, light_sampler, input.light_uv);
    }
    if input.visual_kind == 25u {
        let base = input.local_uv.x > 0.06 && input.local_uv.x < 0.94 &&
            input.local_uv.y > 0.72 && input.local_uv.y < 0.93;
        let cabinet = input.local_uv.x > 0.16 && input.local_uv.x < 0.84 &&
            input.local_uv.y > 0.40 && input.local_uv.y <= 0.76;
        let screen = input.local_uv.x > 0.29 && input.local_uv.x < 0.71 &&
            input.local_uv.y > 0.47 && input.local_uv.y < 0.65;
        let mast = input.local_uv.x > 0.47 && input.local_uv.x < 0.53 &&
            input.local_uv.y > 0.17 && input.local_uv.y <= 0.42;
        let dish_delta = (input.local_uv - vec2<f32>(0.5, 0.20)) * vec2<f32>(1.0, 2.2);
        let dish = length(dish_delta) < 0.17 && input.local_uv.y < 0.23;
        let foot = (input.local_uv.x > 0.19 && input.local_uv.x < 0.29 ||
            input.local_uv.x > 0.71 && input.local_uv.x < 0.81) &&
            input.local_uv.y >= 0.91;
        if !base && !cabinet && !mast && !dish && !foot {
            discard;
        }
        var colour = vec3<f32>(0.12, 0.15, 0.17);
        if cabinet {
            colour = vec3<f32>(0.22, 0.28, 0.30);
        }
        if screen {
            colour = vec3<f32>(0.02, 0.72, 0.68);
        }
        if mast || dish {
            colour = vec3<f32>(0.48, 0.55, 0.57);
        }
        if screen {
            return vec4<f32>(colour, 1.0);
        }
        return vec4<f32>(colour, 1.0) *
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
