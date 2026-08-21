use wgpu::{VertexAttribute, VertexBufferLayout, VertexFormat, VertexStepMode};

pub struct BufferLayout {
    attributes: Vec<VertexAttribute>,
    stride: u64,
    step_mode: VertexStepMode,
}

impl Default for BufferLayout {
    fn default() -> Self {
        Self::new()
    }
}

impl BufferLayout {
    pub fn new() -> Self {
        Self {
            attributes: vec![],
            stride: 0,
            step_mode: VertexStepMode::Vertex,
        }
    }

    pub fn attribute(mut self, shader_location: u32, offset: u64, format: VertexFormat) -> Self {
        self.attributes.push(VertexAttribute {
            format,
            offset,
            shader_location,
        });
        self
    }

    pub fn stride(mut self, stride: u64) -> Self {
        self.stride = stride;
        self
    }
    pub fn step_mode(mut self, mode: VertexStepMode) -> Self {
        self.step_mode = mode;
        self
    }

    pub(crate) fn to_wgpu_layout(&self) -> wgpu::VertexBufferLayout<'static> {
        let attrs: &'static [wgpu::VertexAttribute] =
            Box::leak(self.attributes.clone().into_boxed_slice());
        VertexBufferLayout {
            array_stride: self.stride,
            step_mode: self.step_mode,
            attributes: attrs,
        }
    }
}

pub trait GpuInstance: bytemuck::Pod + bytemuck::Zeroable {
    fn buffer_layout() -> BufferLayout;
}

pub trait GpuVertex: bytemuck::Pod + bytemuck::Zeroable {
    fn buffer_layout() -> BufferLayout;
}
