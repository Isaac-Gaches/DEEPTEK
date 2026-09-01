//! Fullscreen procedural sky, clouds, stars, and day/night ambient light.

use crate::render_common;
use easy_gpu::assets::{
    Buffer, BufferLayout, BufferUsages, GpuInstance, GpuVertex, Material, MaterialBuilder, Mesh,
    RenderPipelineBuilder, render_uniform,
};
use easy_gpu::assets_manager::Handle;
use easy_gpu::frame::Frame;
use easy_gpu::wgpu::{TextureFormat, VertexFormat, VertexStepMode};

const DEFAULT_CYCLE_SECONDS: f32 = 500.0;
const DEFAULT_INITIAL_TIME: f32 = 0.42;
const DEFAULT_STAR_COUNT: u32 = 1_000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyRenderConfig {
    /// Real seconds taken for one complete in-game day.
    pub cycle_seconds: f32,
    /// Normalized initial time in the range `0.0..1.0`.
    pub initial_time: f32,
    pub star_count: u32,
}

impl Default for SkyRenderConfig {
    fn default() -> Self {
        Self {
            cycle_seconds: DEFAULT_CYCLE_SECONDS,
            initial_time: DEFAULT_INITIAL_TIME,
            star_count: DEFAULT_STAR_COUNT,
        }
    }
}

/// Owns the GPU resources and clock for the procedural sky.
pub struct SkyRenderer {
    cycle_seconds: f32,
    time_of_day: f32,
    cloud_elapsed_seconds: f64,
    ambient_light: [f32; 3],
    quad: Handle<Mesh>,
    sky_material: Handle<Material>,
    sky_uniform: Handle<Buffer>,
    star_material: Handle<Material>,
    star_uniform: Handle<Buffer>,
    star_buffer: Handle<Buffer>,
    star_count: u32,
}

impl SkyRenderer {
    pub fn new(gpu: &mut easy_gpu::Renderer) -> Self {
        Self::with_config(gpu, SkyRenderConfig::default())
    }

    pub fn with_config(gpu: &mut easy_gpu::Renderer, config: SkyRenderConfig) -> Self {
        let cycle_seconds = finite_positive_or(config.cycle_seconds, DEFAULT_CYCLE_SECONDS);
        let time_of_day = normalize_time(config.initial_time);
        let palette = SkyPalette::at(time_of_day);
        let aspect_ratio = gpu.window_aspect().max(0.01);
        let uniform = SkyUniform::new(time_of_day, aspect_ratio, 0.0, palette);

        let quad = render_common::create_unit_quad(gpu);
        let sky_shader = gpu.load_shader(include_str!("sky.wgsl"));
        let sky_pipeline = RenderPipelineBuilder::new(sky_shader)
            .depth_writes_enabled(false)
            .depth_format(TextureFormat::Depth24Plus)
            .vertex_layout(render_common::QuadVertex::buffer_layout())
            .material_layout(&[render_uniform(0)])
            .build(gpu);
        let sky_uniform = gpu.create_buffer_with_contents(
            BufferUsages::COPY_DST | BufferUsages::UNIFORM,
            bytemuck::bytes_of(&uniform),
        );
        let sky_material = MaterialBuilder::new(sky_pipeline)
            .buffer(0, sky_uniform)
            .build(gpu);
        let star_shader = gpu.load_shader(include_str!("stars.wgsl"));
        let star_pipeline = RenderPipelineBuilder::new(star_shader)
            .depth_writes_enabled(false)
            .depth_format(TextureFormat::Depth24Plus)
            .vertex_layout(render_common::QuadVertex::buffer_layout())
            .vertex_layout(Star::buffer_layout())
            .material_layout(&[render_uniform(0)])
            .additive_alpha_blending()
            .build(gpu);
        let star_uniform_value = StarUniform {
            time_of_day,
            aspect_ratio,
            _padding: [0.0; 2],
        };
        let star_uniform = gpu.create_buffer_with_contents(
            BufferUsages::COPY_DST | BufferUsages::UNIFORM,
            bytemuck::bytes_of(&star_uniform_value),
        );
        let star_material = MaterialBuilder::new(star_pipeline)
            .buffer(0, star_uniform)
            .build(gpu);

        // Star placement is deterministic so recreating a renderer preserves the sky.
        let star_count = config.star_count.max(1);
        let stars: Vec<_> = (0..star_count)
            .map(|index| Star {
                position: [star_coordinate(index, 0), star_coordinate(index, 1)],
            })
            .collect();
        let star_buffer =
            gpu.create_buffer_with_contents(BufferUsages::VERTEX, bytemuck::cast_slice(&stars));

        Self {
            cycle_seconds,
            time_of_day,
            cloud_elapsed_seconds: 0.0,
            ambient_light: palette.light,
            quad,
            sky_material,
            sky_uniform,
            star_material,
            star_uniform,
            star_buffer,
            star_count,
        }
    }

    pub fn update(&mut self, gpu: &easy_gpu::Renderer, elapsed_seconds: f32) {
        advance_clocks(
            &mut self.time_of_day,
            &mut self.cloud_elapsed_seconds,
            elapsed_seconds,
            self.cycle_seconds,
        );
        self.write_uniforms(gpu);
    }

    pub fn set_time_of_day(&mut self, gpu: &easy_gpu::Renderer, time_of_day: f32) {
        self.time_of_day = normalize_time(time_of_day);
        self.write_uniforms(gpu);
    }

    pub const fn time_of_day(&self) -> f32 {
        self.time_of_day
    }

    pub const fn ambient_light(&self) -> [f32; 3] {
        self.ambient_light
    }

    pub fn draw(&self, frame: &mut Frame) {
        frame.draw(self.sky_material, self.quad);
        frame.draw_manual_batch(
            vec![self.star_buffer],
            self.star_material,
            self.quad,
            0..self.star_count,
        );
    }

    fn write_uniforms(&mut self, gpu: &easy_gpu::Renderer) {
        let palette = SkyPalette::at(self.time_of_day);
        let aspect_ratio = gpu.window_aspect().max(0.01);
        self.ambient_light = palette.light;
        gpu.write_buffer(
            self.sky_uniform,
            SkyUniform::new(
                self.time_of_day,
                aspect_ratio,
                self.cloud_elapsed_seconds as f32,
                palette,
            ),
        );
        gpu.write_buffer(
            self.star_uniform,
            StarUniform {
                time_of_day: self.time_of_day,
                aspect_ratio,
                _padding: [0.0; 2],
            },
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct SkyPalette {
    top: [f32; 3],
    horizon: [f32; 3],
    light: [f32; 3],
    cloud_main: [f32; 3],
    cloud_edge: [f32; 3],
}

impl SkyPalette {
    const DAY: Self = Self {
        top: [0.22, 0.55, 0.95],
        horizon: [0.65, 0.80, 1.00],
        light: [1.0, 1.0, 1.0],
        cloud_main: [1.0, 1.0, 1.0],
        cloud_edge: [0.3, 0.7, 1.0],
    };
    const LOW_SUN: Self = Self {
        top: [0.5, 0.05, 0.20],
        horizon: [0.7, 0.3, 0.10],
        light: [0.8, 0.4, 0.1],
        cloud_main: [1.0, 0.4, 0.8],
        cloud_edge: [0.6, 0.2, 0.1],
    };
    const NIGHT: Self = Self {
        top: [0.0, 0.0, 0.0],
        horizon: [0.002, 0.001, 0.004],
        light: [0.01, 0.01, 0.01],
        cloud_main: [0.08, 0.08, 0.1],
        cloud_edge: [0.05, 0.05, 0.1],
    };

    fn at(time: f32) -> Self {
        const DAWN_START: f32 = 0.15;
        const SUNRISE_END: f32 = 0.25;
        const DAY_START: f32 = 0.30;
        const SUNSET_START: f32 = 0.70;
        const DUSK_START: f32 = 0.75;
        const NIGHT_START: f32 = 0.85;

        match time {
            time if time < DAWN_START => Self::NIGHT,
            time if time < SUNRISE_END => {
                Self::NIGHT.lerp(Self::LOW_SUN, inverse_lerp(DAWN_START, SUNRISE_END, time))
            }
            time if time < DAY_START => {
                Self::LOW_SUN.lerp(Self::DAY, inverse_lerp(SUNRISE_END, DAY_START, time))
            }
            time if time < SUNSET_START => Self::DAY,
            time if time < DUSK_START => {
                Self::DAY.lerp(Self::LOW_SUN, inverse_lerp(SUNSET_START, DUSK_START, time))
            }
            time if time < NIGHT_START => {
                Self::LOW_SUN.lerp(Self::NIGHT, inverse_lerp(DUSK_START, NIGHT_START, time))
            }
            _ => Self::NIGHT,
        }
    }

    fn lerp(self, other: Self, amount: f32) -> Self {
        Self {
            top: lerp_colour(self.top, other.top, amount),
            horizon: lerp_colour(self.horizon, other.horizon, amount),
            light: lerp_colour(self.light, other.light, amount),
            cloud_main: lerp_colour(self.cloud_main, other.cloud_main, amount),
            cloud_edge: lerp_colour(self.cloud_edge, other.cloud_edge, amount),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SkyUniform {
    time_of_day: f32,
    aspect_ratio: f32,
    cloud_elapsed_seconds: f32,
    _padding0: f32,
    top_colour: [f32; 3],
    _padding1: f32,
    bottom_colour: [f32; 3],
    _padding2: f32,
    cloud_main: [f32; 3],
    _padding3: f32,
    cloud_edge: [f32; 3],
    _padding4: f32,
}

impl SkyUniform {
    fn new(
        time_of_day: f32,
        aspect_ratio: f32,
        cloud_elapsed_seconds: f32,
        palette: SkyPalette,
    ) -> Self {
        Self {
            time_of_day,
            aspect_ratio,
            cloud_elapsed_seconds,
            _padding0: 0.0,
            top_colour: palette.top,
            _padding1: 0.0,
            bottom_colour: palette.horizon,
            _padding2: 0.0,
            cloud_main: palette.cloud_main,
            _padding3: 0.0,
            cloud_edge: palette.cloud_edge,
            _padding4: 0.0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct StarUniform {
    time_of_day: f32,
    aspect_ratio: f32,
    _padding: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Star {
    position: [f32; 2],
}

impl GpuInstance for Star {
    fn buffer_layout() -> BufferLayout {
        BufferLayout::new()
            .stride(size_of::<Self>() as u64)
            .step_mode(VertexStepMode::Instance)
            .attribute(1, 0, VertexFormat::Float32x2)
    }
}

fn finite_positive_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    }
}

fn normalize_time(value: f32) -> f32 {
    if value.is_finite() {
        value.rem_euclid(1.0)
    } else {
        DEFAULT_INITIAL_TIME
    }
}

fn advance_clocks(
    time_of_day: &mut f32,
    cloud_elapsed_seconds: &mut f64,
    elapsed_seconds: f32,
    cycle_seconds: f32,
) {
    if elapsed_seconds.is_finite() && elapsed_seconds > 0.0 {
        *time_of_day = (*time_of_day + elapsed_seconds / cycle_seconds).rem_euclid(1.0);
        *cloud_elapsed_seconds += f64::from(elapsed_seconds);
    }
}

fn inverse_lerp(start: f32, end: f32, value: f32) -> f32 {
    (value - start) / (end - start)
}

fn lerp_colour(a: [f32; 3], b: [f32; 3], amount: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * amount,
        a[1] + (b[1] - a[1]) * amount,
        a[2] + (b[2] - a[2]) * amount,
    ]
}

fn star_coordinate(index: u32, axis: u32) -> f32 {
    let mut value = u64::from(index) | (u64::from(axis) << 32);
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^= value >> 31;
    let unit = ((value >> 40) as u32) as f32 / ((1_u32 << 24) - 1) as f32;
    unit * 2.0 - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_preserves_legacy_day_and_night_light() {
        assert_eq!(SkyPalette::at(0.5).light, SkyPalette::DAY.light);
        assert_eq!(SkyPalette::at(0.0).light, SkyPalette::NIGHT.light);
        assert_eq!(SkyPalette::at(0.95).light, SkyPalette::NIGHT.light);
    }

    #[test]
    fn sunrise_light_blends_between_night_and_low_sun() {
        let light = SkyPalette::at(0.20).light;
        assert!((light[0] - 0.405).abs() < 0.000_01);
        assert!((light[1] - 0.205).abs() < 0.000_01);
        assert!((light[2] - 0.055).abs() < 0.000_01);
    }

    #[test]
    fn normalized_time_wraps_in_both_directions() {
        assert!((normalize_time(1.25) - 0.25).abs() < f32::EPSILON);
        assert!((normalize_time(-0.25) - 0.75).abs() < f32::EPSILON);
    }

    #[test]
    fn deterministic_stars_stay_in_clip_space() {
        for index in 0..DEFAULT_STAR_COUNT {
            for axis in 0..2 {
                assert!((-1.0..=1.0).contains(&star_coordinate(index, axis)));
            }
        }
    }

    #[test]
    fn uniforms_match_their_wgsl_alignment() {
        assert_eq!(size_of::<SkyUniform>(), 80);
        assert_eq!(size_of::<StarUniform>(), 16);
    }

    #[test]
    fn cloud_clock_is_independent_from_the_wrapping_day_clock() {
        let mut time_of_day = 0.99;
        let mut cloud_elapsed_seconds = 120.0_f64;
        advance_clocks(
            &mut time_of_day,
            &mut cloud_elapsed_seconds,
            10.0,
            DEFAULT_CYCLE_SECONDS,
        );

        assert!((time_of_day - 0.01).abs() < 0.000_01);
        assert_eq!(cloud_elapsed_seconds, 130.0);
    }
}
