use std::num::NonZero;

use wgpu::{Adapter, Device, Instance, InstanceFlags, MemoryBudgetThresholds, Queue, RenderPipeline};


#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TransitionUniform {
    pub resolution: [f32; 2],
    pub origin: [f32; 2],
    pub angle: f32,
    pub progress: f32,
    pub effect: u32,
    pub has_from: u32
}

pub struct GpuContext {
    pub instance: Instance,
    adapter: Adapter,
    pub device: Device,
    pub queue: Queue,
    render_pipeline: RenderPipeline
}
// clippy::new_ret_no_self
impl GpuContext {
    pub fn new() -> anyhow::Result<Self> {
        let instance = Self::new_instance();
        let adapter = Self::new_adapter(&instance)?;
        let (device, queue) = pollster::block_on(
            adapter.request_device(&wgpu::DeviceDescriptor {
                ..Default::default()
            }))?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into())
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("transition_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { 
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(NonZero::new(std::mem::size_of::<TransitionUniform>() as u64).unwrap()) },
                    count: None
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true }, 
                        view_dimension: wgpu::TextureViewDimension::D2, 
                        multisampled: false
                    },
                    count: None
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { 
                        sample_type: wgpu::TextureSampleType::Float { filterable: true } ,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None
                }
            ]
        });
        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { 
            label: Some("render_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor { 
            label: Some("render_pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[
                    Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Bgra8Unorm,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL
                    })
                ],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false
            },
            multiview_mask: None,
            cache: None
        });
        Ok(Self { instance, adapter, device, queue, render_pipeline })
    }

    fn new_instance() -> wgpu::Instance {
        Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            flags: InstanceFlags::default(),
            memory_budget_thresholds: MemoryBudgetThresholds::default(),
            backend_options: wgpu::BackendOptions::default(),
            display: Option::default()
        })
    }

    fn new_adapter(instance: &Instance) -> anyhow::Result<Adapter> {
        Ok(pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptionsBase {
            power_preference: wgpu::PowerPreference::HighPerformance, 
            force_fallback_adapter: false,
            compatible_surface: None,
            apply_limit_buckets: false
        }))?)
    }
}
