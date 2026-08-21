use easy_gpu::assets::{BufferLayout, GpuVertex, Mesh};
use easy_gpu::assets_manager::Handle;
use easy_gpu::wgpu::VertexFormat;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct QuadVertex {
    position: [f32; 2],
}

impl GpuVertex for QuadVertex {
    fn buffer_layout() -> BufferLayout {
        BufferLayout::new()
            .stride(size_of::<Self>() as u64)
            .attribute(0, 0, VertexFormat::Float32x2)
    }
}

pub(crate) fn create_unit_quad(gpu: &mut easy_gpu::Renderer) -> Handle<Mesh> {
    gpu.create_mesh(
        &[
            QuadVertex {
                position: [-0.5, -0.5],
            },
            QuadVertex {
                position: [0.5, -0.5],
            },
            QuadVertex {
                position: [0.5, 0.5],
            },
            QuadVertex {
                position: [-0.5, 0.5],
            },
        ],
        &[0, 1, 2, 0, 2, 3],
    )
}
