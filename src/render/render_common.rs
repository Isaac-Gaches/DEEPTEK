use easy_gpu::assets::{BufferLayout, GpuVertex, Material, MaterialBuilder, Mesh};
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

/// Rebuilds a material's bind group while preserving its stable handle.
pub(crate) fn replace_material(
    gpu: &mut easy_gpu::Renderer,
    target: Handle<Material>,
    builder: MaterialBuilder,
) {
    let replacement_handle = builder.build(gpu);
    let replacement = gpu
        .asset_manager
        .materials
        .remove(replacement_handle)
        .expect("newly built material remains present");
    *gpu.asset_manager
        .materials
        .get_mut(target)
        .expect("material being rebound remains present") = replacement;
}
