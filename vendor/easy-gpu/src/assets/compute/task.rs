use crate::assets::compute::bind_group::ComputeBindGroup;
use crate::assets::compute::pipeline::ComputePipeline;
use crate::assets_manager::Handle;
use crate::assets_manager::asset_manager::AssetManager;
use wgpu::CommandEncoder;

pub(crate) struct ComputeTask {
    pub(crate) pipeline: Handle<ComputePipeline>,
    bind_group: Handle<ComputeBindGroup>,
    dispatch: (u32, u32, u32),
}

impl ComputeTask {
    pub fn new(
        pipeline: Handle<ComputePipeline>,
        bind_group: Handle<ComputeBindGroup>,
        dispatch: (u32, u32, u32),
    ) -> Self {
        Self {
            pipeline,
            bind_group,
            dispatch,
        }
    }

    pub fn execute(&self, encoder: &mut CommandEncoder, asset_manager: &AssetManager) {
        let compute_pipeline = asset_manager.compute_pipelines.get(self.pipeline).unwrap();
        let compute_bind_group = asset_manager
            .compute_bind_groups
            .get(self.bind_group)
            .unwrap();

        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: None,
            timestamp_writes: None,
        });
        pass.set_pipeline(&compute_pipeline.pipeline);
        pass.set_bind_group(0, &compute_bind_group.bind_group, &[]);
        pass.dispatch_workgroups(self.dispatch.0, self.dispatch.1, self.dispatch.2);
    }
}
