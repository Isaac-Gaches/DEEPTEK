use std::ops::Range;

use crate::assets::buffer::Buffer;
use crate::assets::compute::bind_group::ComputeBindGroup;
use crate::assets::compute::pipeline::ComputePipeline;
use crate::assets::compute::task::ComputeTask;
use crate::assets::render::material::Material;
use crate::assets::render::mesh::Mesh;
use crate::assets::render::task::{DrawCommand, InstancedCommand, RenderTask, StreamedCommand};
use crate::assets::{GpuInstance, Texture};
use crate::assets_manager::handle::Handle;

pub struct Frame {
    pub(crate) render_tasks: Vec<RenderTask>,
    pub(crate) texture_render_passes: Vec<TextureRenderPass>,
    pub(crate) compute_tasks: Vec<ComputeTask>,
    pub(crate) textures_to_clear: Vec<Handle<Texture>>,
    pub(crate) instance_bytes: Vec<u8>,
}

pub(crate) struct TextureRenderPass {
    pub(crate) target: Handle<Texture>,
    pub(crate) use_depth: bool,
    pub(crate) render_tasks: Vec<RenderTask>,
}

impl Frame {
    pub(crate) fn new() -> Self {
        Self {
            render_tasks: Vec::new(),
            texture_render_passes: Vec::new(),
            compute_tasks: Vec::new(),
            textures_to_clear: Vec::new(),
            instance_bytes: Vec::new(),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.render_tasks.clear();
        self.texture_render_passes.clear();
        self.compute_tasks.clear();
        self.instance_bytes.clear();
    }

    pub fn draw(&mut self, material: Handle<Material>, mesh: Handle<Mesh>) {
        self.render_tasks
            .push(RenderTask::Draw(DrawCommand { mesh, material }));
    }

    pub fn draw_manual_batch(
        &mut self,
        instances: Vec<Handle<Buffer>>,
        material: Handle<Material>,
        mesh: Handle<Mesh>,
        range: Range<u32>,
    ) {
        self.render_tasks
            .push(RenderTask::DrawStreamed(StreamedCommand {
                mesh,
                material,
                instances,
                range,
            }));
    }

    pub fn draw_batch<T: GpuInstance>(
        &mut self,
        instances: &[T],
        material: Handle<Material>,
        mesh: Handle<Mesh>,
    ) {
        let instance_count = instances.len() as u32;
        let start = self.instance_bytes.len();
        self.instance_bytes
            .extend_from_slice(bytemuck::cast_slice(instances));
        let end = self.instance_bytes.len();

        self.render_tasks
            .push(RenderTask::DrawInstanced(InstancedCommand {
                mesh,
                material,
                instance_range: start as u64..end as u64,
                instance_count,
            }));
    }

    /// Finishes the current group of draw calls into `target` instead of the
    /// surface. Later draw calls form a new pass. This keeps post-processing
    /// chains in one command encoder and one queue submission.
    pub fn render_to_texture(&mut self, target: Handle<Texture>, use_depth: bool) {
        self.texture_render_passes.push(TextureRenderPass {
            target,
            use_depth,
            render_tasks: std::mem::take(&mut self.render_tasks),
        });
    }

    pub fn compute(
        &mut self,
        bind_group: Handle<ComputeBindGroup>,
        pipeline: Handle<ComputePipeline>,
        dispatch: (u32, u32, u32),
    ) {
        self.compute_tasks
            .push(ComputeTask::new(pipeline, bind_group, dispatch));
    }

    pub fn sort_by_material(&mut self) {
        self.render_tasks.sort_by_key(|item| match item {
            RenderTask::Draw(command) => command.material.index,
            RenderTask::DrawInstanced(command) => command.material.index,
            RenderTask::DrawStreamed(command) => command.material.index,
        });
    }

    pub fn sort_by_mesh(&mut self) {
        self.render_tasks.sort_by_key(|item| match item {
            RenderTask::Draw(command) => command.mesh.index,
            RenderTask::DrawInstanced(command) => command.mesh.index,
            RenderTask::DrawStreamed(command) => command.mesh.index,
        });
    }

    pub fn sort(&mut self) {
        self.render_tasks.sort_by_key(|item| match item {
            RenderTask::Draw(command) => (command.material.index, command.mesh.index),
            RenderTask::DrawInstanced(command) => (command.material.index, command.mesh.index),
            RenderTask::DrawStreamed(command) => (command.material.index, command.mesh.index),
        });
    }

    pub fn request_texture_clear(&mut self, texture: Handle<Texture>) {
        self.textures_to_clear.push(texture);
    }
}
