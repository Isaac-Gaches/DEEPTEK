use crate::assets::{Buffer, Material, Mesh};
use crate::assets_manager::Handle;
use std::ops::Range;

pub enum RenderTask {
    Draw(DrawCommand),
    DrawInstanced(InstancedCommand),
    DrawStreamed(StreamedCommand),
}

pub(crate) struct InstancedCommand {
    pub mesh: Handle<Mesh>,
    pub material: Handle<Material>,
    pub instance_range: Range<u64>,
    pub instance_count: u32,
}

pub(crate) struct StreamedCommand {
    pub mesh: Handle<Mesh>,
    pub material: Handle<Material>,
    pub instances: Vec<Handle<Buffer>>,
    pub range: Range<u32>,
}

pub(crate) struct DrawCommand {
    pub mesh: Handle<Mesh>,
    pub material: Handle<Material>,
}
