use super::font;
use crate::{SpriteAtlasFrame, render_common};
use easy_gpu::assets::{
    Buffer, BufferLayout, BufferUsages, GpuInstance, GpuVertex, Material, MaterialBuilder, Mesh,
    RenderPipeline, RenderPipelineBuilder, SamplerBuilder, Texture, TextureBuilder, render_storage,
    render_texture, render_uniform, sampler,
};
use easy_gpu::assets_manager::Handle;
use easy_gpu::frame::Frame;
use easy_gpu::wgpu::{
    BlendState, Extent3d, FilterMode, TextureFormat, VertexFormat, VertexStepMode,
};
use std::collections::HashMap;

const ITEM_ICON_FRAMES: u32 = 14;
const ROPE_ICON_FRAME: u32 = ITEM_ICON_FRAMES;
const UTILITY_ICON_FIRST_FRAME: u32 = ROPE_ICON_FRAME + 1;
const UTILITY_ICON_FRAMES: u32 = 4;
pub(super) const WORLD_MAP_TEXTURE_SIZE: u32 = 512;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GuiInstance {
    position: [f32; 2],
    size: [f32; 2],
    depth: f32,
    frame: u32,
    tint: [f32; 4],
}

impl GpuInstance for GuiInstance {
    fn buffer_layout() -> BufferLayout {
        BufferLayout::new()
            .stride(size_of::<Self>() as u64)
            .step_mode(VertexStepMode::Instance)
            .attribute(1, 0, VertexFormat::Float32x2)
            .attribute(2, 8, VertexFormat::Float32x2)
            .attribute(3, 16, VertexFormat::Float32)
            .attribute(4, 20, VertexFormat::Uint32)
            .attribute(5, 24, VertexFormat::Float32x4)
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GuiUniform {
    viewport: [f32; 2],
    _padding: [f32; 2],
}

pub struct GuiRenderer {
    quad: Handle<Mesh>,
    uniform: Handle<Buffer>,
    panel_material: Handle<Material>,
    world_map_material: Handle<Material>,
    world_map_overlay_material: Handle<Material>,
    world_map_atlas: Handle<Buffer>,
    world_map_texture: Handle<Texture>,
    slot_material: Handle<Material>,
    icon_material: Handle<Material>,
    rope_icon_material: Handle<Material>,
    utility_icon_material: Handle<Material>,
    text_material: Handle<Material>,
    font_atlas: font::FontAtlas,
    batches: HashMap<Handle<Material>, Vec<GuiInstance>>,
}

impl GuiRenderer {
    pub fn new(gpu: &mut easy_gpu::Renderer) -> Self {
        let quad = render_common::create_unit_quad(gpu);
        let uniform = gpu.create_buffer(
            BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            size_of::<GuiUniform>() as u64,
        );
        let pipeline = RenderPipelineBuilder::new(gpu.load_shader(include_str!("shader.wgsl")))
            .material_layout(&[
                render_texture(0),
                sampler(1),
                render_storage(2, true),
                render_uniform(3),
            ])
            .vertex_layout(render_common::QuadVertex::buffer_layout())
            .vertex_layout(GuiInstance::buffer_layout())
            .depth_format(TextureFormat::Depth24Plus)
            .depth_writes_enabled(false)
            .blend_mode(BlendState::ALPHA_BLENDING)
            .build(gpu);
        let sampler = SamplerBuilder::new()
            .filter_mode(FilterMode::Nearest)
            .build(gpu);
        let text_sampler = SamplerBuilder::new()
            .filter_mode(FilterMode::Linear)
            .build(gpu);

        let slot_texture =
            gpu.load_texture_from_file(include_bytes!("../../assets/gui/slot.png").to_vec());
        let icon_texture = gpu.load_texture_from_file(
            include_bytes!("../../assets/gui/items_with_power.png").to_vec(),
        );
        let panel_texture = create_solid_texture(gpu);
        let world_map_texture = create_world_map_texture(gpu);
        let font_atlas = font::FontAtlas::new(gpu);
        let rope_icon_texture = create_rope_icon_texture(gpu);
        let utility_icon_texture = create_utility_icon_texture(gpu);
        let panel_material = create_material(
            gpu,
            pipeline,
            sampler,
            uniform,
            panel_texture,
            &[SpriteAtlasFrame::FULL],
        );
        let world_map_atlas = gpu.create_buffer_with_contents(
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
            bytemuck::cast_slice(&[SpriteAtlasFrame::FULL]),
        );
        gpu.write_buffer(world_map_atlas, SpriteAtlasFrame::FULL);
        let world_map_material = MaterialBuilder::new(pipeline)
            .texture(0, world_map_texture)
            .sampler(1, sampler)
            .buffer(2, world_map_atlas)
            .buffer(3, uniform)
            .build(gpu);
        let world_map_overlay_material = create_material(
            gpu,
            pipeline,
            sampler,
            uniform,
            panel_texture,
            &[SpriteAtlasFrame::FULL],
        );
        let slot_material = create_material(
            gpu,
            pipeline,
            sampler,
            uniform,
            slot_texture,
            &[SpriteAtlasFrame::FULL],
        );
        let icon_frames: Vec<_> = (0..ITEM_ICON_FRAMES)
            .map(|frame| horizontal_frame(frame, ITEM_ICON_FRAMES))
            .collect();
        let icon_material =
            create_material(gpu, pipeline, sampler, uniform, icon_texture, &icon_frames);
        let rope_icon_material = create_material(
            gpu,
            pipeline,
            sampler,
            uniform,
            rope_icon_texture,
            &[SpriteAtlasFrame::FULL],
        );
        let utility_icon_frames: Vec<_> = (0..UTILITY_ICON_FRAMES)
            .map(|frame| horizontal_frame(frame, UTILITY_ICON_FRAMES))
            .collect();
        let utility_icon_material = create_material(
            gpu,
            pipeline,
            sampler,
            uniform,
            utility_icon_texture,
            &utility_icon_frames,
        );
        let text_material = create_material(
            gpu,
            pipeline,
            text_sampler,
            uniform,
            font_atlas.texture,
            &font_atlas.frames,
        );

        Self {
            quad,
            uniform,
            panel_material,
            world_map_material,
            world_map_overlay_material,
            world_map_atlas,
            world_map_texture,
            slot_material,
            icon_material,
            rope_icon_material,
            utility_icon_material,
            text_material,
            font_atlas,
            batches: HashMap::new(),
        }
    }

    pub fn prepare(&mut self, gpu: &easy_gpu::Renderer, viewport: [f32; 2]) {
        gpu.write_buffer(
            self.uniform,
            GuiUniform {
                viewport: [viewport[0].max(1.0), viewport[1].max(1.0)],
                _padding: [0.0; 2],
            },
        );
        for batch in self.batches.values_mut() {
            batch.clear();
        }
    }

    pub fn queue_slot(&mut self, position: [f32; 2], size: f32, tint: [f32; 4]) {
        self.queue(self.slot_material, 0, position, [size; 2], 0.0, tint);
    }

    pub fn queue_slot_rect(&mut self, position: [f32; 2], size: [f32; 2], tint: [f32; 4]) {
        self.queue(self.slot_material, 0, position, size, 0.0, tint);
    }

    pub fn queue_rect(&mut self, position: [f32; 2], size: [f32; 2], tint: [f32; 4]) {
        self.queue(self.panel_material, 0, position, size, 0.0, tint);
    }

    pub fn update_world_map_texture(&self, gpu: &easy_gpu::Renderer, pixels: &[u8]) {
        let extent = Extent3d {
            width: WORLD_MAP_TEXTURE_SIZE,
            height: WORLD_MAP_TEXTURE_SIZE,
            depth_or_array_layers: 1,
        };
        debug_assert_eq!(pixels.len(), (extent.width * extent.height * 4) as usize);
        gpu.write_texture(self.world_map_texture, pixels, 4, extent);
    }

    pub fn update_world_map_view(
        &self,
        gpu: &easy_gpu::Renderer,
        min_uv: [f32; 2],
        max_uv: [f32; 2],
    ) {
        gpu.write_buffer(self.world_map_atlas, SpriteAtlasFrame::new(min_uv, max_uv));
    }

    pub fn queue_world_map(&mut self, position: [f32; 2], size: [f32; 2]) {
        self.queue(self.world_map_material, 0, position, size, 0.0, [1.0; 4]);
    }

    pub fn queue_world_map_overlay(&mut self, position: [f32; 2], size: [f32; 2], tint: [f32; 4]) {
        self.queue(
            self.world_map_overlay_material,
            0,
            position,
            size,
            0.0,
            tint,
        );
    }

    pub fn queue_icon(&mut self, frame: u32, position: [f32; 2], size: f32, tint: [f32; 4]) {
        if frame < ITEM_ICON_FRAMES {
            self.queue(self.icon_material, frame, position, [size; 2], 0.0, tint);
        } else if frame == ROPE_ICON_FRAME {
            self.queue(self.rope_icon_material, 0, position, [size; 2], 0.0, tint);
        } else if (UTILITY_ICON_FIRST_FRAME..UTILITY_ICON_FIRST_FRAME + UTILITY_ICON_FRAMES)
            .contains(&frame)
        {
            self.queue(
                self.utility_icon_material,
                frame - UTILITY_ICON_FIRST_FRAME,
                position,
                [size; 2],
                0.0,
                tint,
            );
        }
    }

    /// Queues proportionally spaced Orbitron text from a pixel-space top-left origin.
    pub fn queue_text(&mut self, text: &str, top_left: [f32; 2], pixel_scale: f32, tint: [f32; 4]) {
        let size = font::TextSize::from_legacy_scale(pixel_scale);
        let line_metrics = font::line_metrics(size);
        let mut cursor_x = top_left[0];
        let mut baseline = top_left[1] + line_metrics.ascent;
        let mut previous = None;
        for character in text.chars() {
            if character == '\n' {
                cursor_x = top_left[0];
                baseline += line_metrics.new_line_size;
                previous = None;
                continue;
            }
            if let Some(previous) = previous {
                cursor_x += font::font()
                    .horizontal_kern(previous, character, size.pixels())
                    .unwrap_or(0.0);
            }
            if let Some(glyph) = self.font_atlas.glyph(size, character) {
                let metrics = glyph.metrics;
                if glyph.frame != u32::MAX && metrics.width > 0 && metrics.height > 0 {
                    let glyph_size = [metrics.width as f32, metrics.height as f32];
                    let glyph_left = cursor_x + metrics.xmin as f32;
                    let glyph_top = baseline - metrics.ymin as f32 - metrics.height as f32;
                    self.queue(
                        self.text_material,
                        glyph.frame,
                        [
                            glyph_left + glyph_size[0] * 0.5,
                            glyph_top + glyph_size[1] * 0.5,
                        ],
                        glyph_size,
                        0.0,
                        tint,
                    );
                }
                cursor_x += metrics.advance_width;
            } else {
                cursor_x += font::font().metrics(character, size.pixels()).advance_width;
            }
            previous = Some(character);
        }
    }

    pub fn text_width(text: &str, pixel_scale: f32) -> f32 {
        font::text_width(text, font::TextSize::from_legacy_scale(pixel_scale))
    }

    pub fn draw(&self, frame: &mut Frame) {
        // GUI has no depth test, so preserve semantic back-to-front ordering.
        for material in [
            self.panel_material,
            self.world_map_material,
            self.slot_material,
            self.icon_material,
            self.rope_icon_material,
            self.utility_icon_material,
            self.world_map_overlay_material,
            self.text_material,
        ] {
            if let Some(instances) = self.batches.get(&material)
                && !instances.is_empty()
            {
                frame.draw_batch(instances, material, self.quad);
            }
        }
    }

    fn queue(
        &mut self,
        material: Handle<Material>,
        frame: u32,
        position: [f32; 2],
        size: [f32; 2],
        depth: f32,
        tint: [f32; 4],
    ) {
        self.batches.entry(material).or_default().push(GuiInstance {
            position,
            size,
            depth,
            frame,
            tint,
        });
    }
}

fn create_material(
    gpu: &mut easy_gpu::Renderer,
    pipeline: Handle<RenderPipeline>,
    sampler: Handle<easy_gpu::assets::Sampler>,
    uniform: Handle<Buffer>,
    texture: Handle<Texture>,
    frames: &[SpriteAtlasFrame],
) -> Handle<Material> {
    let atlas =
        gpu.create_buffer_with_contents(BufferUsages::STORAGE, bytemuck::cast_slice(frames));
    MaterialBuilder::new(pipeline)
        .texture(0, texture)
        .sampler(1, sampler)
        .buffer(2, atlas)
        .buffer(3, uniform)
        .build(gpu)
}

fn horizontal_frame(frame: u32, count: u32) -> SpriteAtlasFrame {
    SpriteAtlasFrame::new(
        [frame as f32 / count as f32, 0.0],
        [(frame + 1) as f32 / count as f32, 1.0],
    )
}

fn create_solid_texture(gpu: &mut easy_gpu::Renderer) -> Handle<Texture> {
    let extent = Extent3d {
        width: 1,
        height: 1,
        depth_or_array_layers: 1,
    };
    let texture = TextureBuilder::new()
        .size(extent)
        .format(TextureFormat::Rgba8UnormSrgb)
        .build(gpu);
    gpu.write_texture(texture, &[255; 4], 4, extent);
    texture
}

fn create_world_map_texture(gpu: &mut easy_gpu::Renderer) -> Handle<Texture> {
    TextureBuilder::new()
        .size(Extent3d {
            width: WORLD_MAP_TEXTURE_SIZE,
            height: WORLD_MAP_TEXTURE_SIZE,
            depth_or_array_layers: 1,
        })
        .format(TextureFormat::Rgba8UnormSrgb)
        .build(gpu)
}

fn create_rope_icon_texture(gpu: &mut easy_gpu::Renderer) -> Handle<Texture> {
    const SIZE: u32 = 9;
    let mut pixels = vec![0_u8; (SIZE * SIZE * 4) as usize];
    for y in 0..SIZE as usize {
        let centre = if (y / 2).is_multiple_of(2) { 4 } else { 5 };
        for (offset, colour) in [
            (-1_i32, [86, 42, 15, 255]),
            (0, [188, 111, 39, 255]),
            (1, [116, 61, 19, 255]),
        ] {
            let x = (centre + offset) as usize;
            let pixel = (y * SIZE as usize + x) * 4;
            pixels[pixel..pixel + 4].copy_from_slice(&colour);
        }
    }
    let extent = Extent3d {
        width: SIZE,
        height: SIZE,
        depth_or_array_layers: 1,
    };
    let texture = TextureBuilder::new()
        .size(extent)
        .format(TextureFormat::Rgba8UnormSrgb)
        .build(gpu);
    gpu.write_texture(texture, &pixels, 4, extent);
    texture
}

fn create_utility_icon_texture(gpu: &mut easy_gpu::Renderer) -> Handle<Texture> {
    const FRAME_SIZE: usize = 9;
    const WIDTH: usize = FRAME_SIZE * UTILITY_ICON_FRAMES as usize;
    let mut pixels = vec![0_u8; WIDTH * FRAME_SIZE * 4];
    let mut set = |frame: usize, x: usize, y: usize, colour: [u8; 4]| {
        let pixel = (y * WIDTH + frame * FRAME_SIZE + x) * 4;
        pixels[pixel..pixel + 4].copy_from_slice(&colour);
    };

    for y in 0..FRAME_SIZE {
        set(0, 3, y, [42, 65, 72, 255]);
        set(0, 4, y, [0, 255, 255, 255]);
        set(0, 5, y, [31, 45, 52, 255]);
    }
    for y in 2..=6 {
        for x in 2..=6 {
            let edge = x == 2 || x == 6 || y == 2 || y == 6;
            set(
                1,
                x,
                y,
                if edge {
                    [44, 52, 58, 255]
                } else {
                    [122, 136, 140, 255]
                },
            );
        }
    }
    set(1, 4, 4, [0, 255, 255, 255]);
    for y in 1..=7 {
        for x in 1..=7 {
            let edge = x == 1 || x == 7 || y == 1 || y == 7;
            set(
                2,
                x,
                y,
                if edge {
                    [34, 42, 48, 255]
                } else {
                    [104, 116, 120, 255]
                },
            );
        }
    }
    for y in 3..=7 {
        set(2, 4, y, [45, 55, 61, 255]);
    }
    set(2, 6, 2, [255, 255, 0, 255]);
    for y in 1..=7 {
        for x in 1..=7 {
            let edge = x == 1 || x == 7 || y == 1 || y == 7;
            let divider = y == 5;
            set(
                3,
                x,
                y,
                if edge || divider {
                    [28, 39, 44, 255]
                } else {
                    [94, 110, 114, 255]
                },
            );
        }
    }
    for x in 3..=5 {
        set(3, x, 3, [0, 255, 255, 255]);
    }

    let extent = Extent3d {
        width: WIDTH as u32,
        height: FRAME_SIZE as u32,
        depth_or_array_layers: 1,
    };
    let texture = TextureBuilder::new()
        .size(extent)
        .format(TextureFormat::Rgba8UnormSrgb)
        .build(gpu);
    gpu.write_texture(texture, &pixels, 4, extent);
    texture
}
