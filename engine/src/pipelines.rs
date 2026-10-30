//! All compute and render pipelines, built once per device.

use crate::gpu::SIM_FORMAT;

const COMMON: &str = include_str!("shaders/common.wgsl");

pub struct Pipelines {
    pub flux: wgpu::ComputePipeline,
    pub transport: wgpu::ComputePipeline,
    pub settle_water: wgpu::ComputePipeline,
    pub settle_pigment: wgpu::ComputePipeline,
    pub bake: wgpu::ComputePipeline,
    pub wet_tiles: wgpu::ComputePipeline,
    pub retire: wgpu::ComputePipeline,
    pub paper_gen: wgpu::ComputePipeline,
    pub paper_init: wgpu::ComputePipeline,
    pub layer_over: wgpu::ComputePipeline,
    pub layer_under_stack: wgpu::ComputePipeline,
    pub final_composite: wgpu::ComputePipeline,
    pub dab_wet: wgpu::RenderPipeline,
    pub dab_fix: wgpu::RenderPipeline,
    pub dab_erase2: wgpu::RenderPipeline,
    pub dab_erase3: wgpu::RenderPipeline,
    pub dab_smear: wgpu::RenderPipeline,
    pub dab_push: wgpu::RenderPipeline,
}

fn module(device: &wgpu::Device, label: &str, src: &str) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(label), source: wgpu::ShaderSource::Wgsl(src.into()) })
}

fn compute(device: &wgpu::Device, m: &wgpu::ShaderModule, entry: &str) -> wgpu::ComputePipeline {
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(entry),
        layout: None,
        module: m,
        entry_point: Some(entry),
        compilation_options: Default::default(),
        cache: None,
    })
}

fn sim(device: &wgpu::Device, label: &str, src: &str) -> wgpu::ComputePipeline {
    let m = module(device, label, &format!("{COMMON}\n{src}"));
    compute(device, &m, "main")
}

pub const DAB_ATTRS: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4];

fn dab_pipeline(device: &wgpu::Device, m: &wgpu::ShaderModule, entry: &str, targets: usize, blend: wgpu::BlendState) -> wgpu::RenderPipeline {
    let t: Vec<Option<wgpu::ColorTargetState>> =
        (0..targets).map(|_| Some(wgpu::ColorTargetState { format: SIM_FORMAT, blend: Some(blend), write_mask: wgpu::ColorWrites::ALL })).collect();
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(entry),
        layout: None,
        vertex: wgpu::VertexState {
            module: m,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<crate::brush::Dab>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &DAB_ATTRS,
            })],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState { module: m, entry_point: Some(entry), compilation_options: Default::default(), targets: &t }),
        multiview_mask: None,
        cache: None,
    })
}

impl Pipelines {
    pub fn new(device: &wgpu::Device) -> Self {
        let comp = module(device, "comp", include_str!("shaders/comp.wgsl"));
        let paper = module(device, "paper_gen", include_str!("shaders/paper_gen.wgsl"));
        let dab = module(device, "dab", include_str!("shaders/dab.wgsl"));
        let add = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add };
        let erase = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add };
        let add = wgpu::BlendState { color: add, alpha: add };
        let erase = wgpu::BlendState { color: erase, alpha: erase };
        Self {
            flux: sim(device, "flux", include_str!("shaders/flux.wgsl")),
            transport: sim(device, "transport", include_str!("shaders/transport.wgsl")),
            settle_water: sim(device, "settle_water", include_str!("shaders/settle_water.wgsl")),
            settle_pigment: sim(device, "settle_pigment", include_str!("shaders/settle_pigment.wgsl")),
            bake: sim(device, "bake", include_str!("shaders/bake.wgsl")),
            wet_tiles: sim(device, "wet_tiles", include_str!("shaders/wet_tiles.wgsl")),
            retire: sim(device, "retire", include_str!("shaders/retire.wgsl")),
            paper_gen: compute(device, &paper, "main"),
            paper_init: compute(device, &comp, "paper_init"),
            layer_over: compute(device, &comp, "layer_over"),
            layer_under_stack: compute(device, &comp, "layer_under_stack"),
            final_composite: compute(device, &comp, "final_composite"),
            dab_wet: dab_pipeline(device, &dab, "fs_wet", 4, add),
            dab_fix: dab_pipeline(device, &dab, "fs_fix", 1, add),
            dab_erase2: dab_pipeline(device, &dab, "fs_erase2", 2, erase),
            dab_erase3: dab_pipeline(device, &dab, "fs_erase3", 3, erase),
            dab_smear: dab_pipeline(device, &dab, "fs_smear", 4, add),
            dab_push: dab_pipeline(device, &dab, "fs_push", 4, add),
        }
    }
}

pub fn present_pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
    let m = module(device, "present", include_str!("shaders/present.wgsl"));
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("present"),
        layout: None,
        vertex: wgpu::VertexState { module: &m, entry_point: Some("vs_main"), compilation_options: Default::default(), buffers: &[] },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &m,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
