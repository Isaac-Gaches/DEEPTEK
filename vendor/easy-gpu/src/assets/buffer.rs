pub use wgpu::BufferUsages;
use wgpu::util::{BufferInitDescriptor, DeviceExt};
use wgpu::{BufferDescriptor, Device};

pub struct Buffer {
    pub(crate) buffer: wgpu::Buffer,
}

impl Buffer {
    pub fn new(device: &Device, size: u64, usage: BufferUsages) -> Self {
        let buffer = device.create_buffer(&BufferDescriptor {
            label: None,
            size,
            usage,
            mapped_at_creation: false,
        });
        Self { buffer }
    }
    pub fn from_contents(device: &Device, contents: &[u8], usage: BufferUsages) -> Self {
        let buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: None,
            contents,
            usage,
        });
        Self { buffer }
    }
}
