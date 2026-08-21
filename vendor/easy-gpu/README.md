# easy-gpu
A simple wgpu wrapper to help manage rendering and compute assets and remove boilerplate code from projects.

## Usage

Example of a simple renderer with a camera and two quads
```rust
let mut egpu = pollster::block_on(easy_gpu::Renderer::new(window.clone()));

let vertices = [
    Vertex::new([-1.0, -1.0]),
    Vertex::new([1.0, -1.0]),
    Vertex::new([1.0, 1.0]),
    Vertex::new([-1.0, 1.0])
];

let indices = [0, 1, 2, 0, 2, 3];

let mesh = egpu.create_mesh(&vertices, &indices);

let camera = Camera::new();

let camera_buffer = egpu.create_buffer_with_contents(
    BufferUsages::UNIFORM | BufferUsages::COPY_DST,
    bytemuck::cast_slice(&[camera])
);

let shader = egpu.load_shader(include_str!("shader.wgsl"));

let pipeline = RenderPipelineBuilder::new(shader.clone())
    .vertex_layout(Vertex::buffer_layout())
    .vertex_layout(Instance::buffer_layout())
    .material_layout(&[uniform(0)])
    .build(&mut egpu);

let material = MaterialBuilder::new(pipeline)
    .uniform(0,camera_buffer)
    .build(&mut egpu);

let instances = vec![
    Instance::new(0.,0.)
    Instance::new(1.,0.)
];

let instance_buffer = egpu.create_buffer_with_contents(
    BufferUsages::VERTEX | BufferUsages::STORAGE,
    bytemuck::cast_slice(instances.as_slice())
);
```
Usage of the above renderer

```rust
let frame = egpu.begin_frame();

frame.draw_instances(
    instance_buffer,
    material,
    mesh,
    0..2
);

egpu.render();
