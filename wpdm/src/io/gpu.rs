use std::{num::NonZero, path::Path, sync::Arc};

use wgpu::{Adapter, BindGroupLayout, Device, Instance, InstanceFlags, MemoryBudgetThresholds, Queue, RenderPipeline, Sampler, Surface, TextureView};
use wpdm_common::TransitionType;

use crate::{io::event::TransitionParams, util::mmap_buffer};

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

/// Per-monitor GPU state: everything needed to draw one output's wallpaper.
/// The texture views are held alive by the bind group, so they are not
/// stored here.
#[derive(Debug)]
pub struct RenderState {
    pub uniform_buf: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub has_from: u32
}

pub struct GpuContext {
    pub instance: Instance,
    adapter: Adapter,
    pub device: Device,
    pub queue: Queue,
    pub render_pipeline: RenderPipeline,
    pub bind_group_layout: BindGroupLayout,
    pub sampler: Sampler,
    pub dummy_view: TextureView
}
// clippy::new_ret_no_self
impl GpuContext {
    pub fn new() -> anyhow::Result<Self> {
        let instance = Self::new_instance();
        let adapter = Self::new_adapter(&instance)?;
        let info = adapter.get_info();
        tracing::info!("Using GPU adapter: {} ({:?}, driver: {})", info.name, info.device_type, info.driver);
        let (device, queue) = pollster::block_on(
            adapter.request_device(&wgpu::DeviceDescriptor {
                ..Default::default()
            }))?;
        // wgpu errors are fatal by default: log them instead, so one bad
        // frame does not take the daemon down.
        device.on_uncaptured_error(Arc::new(|err| {
            tracing::error!("wgpu uncaptured error: {err}");
        }));
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("nearest_clamp_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let dummy_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("dummy_from"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &dummy_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[0u8, 0, 0, 255],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        );
        let dummy_view = dummy_texture.create_view(&wgpu::TextureViewDescriptor::default());

        Ok(Self { instance, adapter, device, queue, render_pipeline, bind_group_layout, sampler, dummy_view })
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

    /// Build the per-monitor GPU state for a wallpaper change: upload the
    /// from/to textures (from may be None on first render), then create the
    /// uniform buffer and bind group. The mmaps are dropped on return:
    /// write_texture stages its own copy.
    pub fn create_render_state(
        &self,
        width: u32,
        height: u32,
        from: Option<&Path>,
        to: &Path,
        params: &TransitionParams,
    ) -> anyhow::Result<RenderState> {
        let expected_len = (width as usize) * (height as usize) * 4;

        let to_pixels = mmap_buffer(to.to_path_buf())?;
        anyhow::ensure!(
            to_pixels.len() == expected_len,
            "wallpaper buffer {to:?} has length {}, expected {expected_len} ({width}x{height}x4)",
            to_pixels.len()
        );

        let (from_view, has_from) = match from {
            Some(path) => {
                let pixels = mmap_buffer(path.to_path_buf())?;
                anyhow::ensure!(
                    pixels.len() == expected_len,
                    "wallpaper buffer {path:?} has length {}, expected {expected_len} ({width}x{height}x4)",
                    pixels.len()
                );
                (self.upload_wallpaper(&pixels, width, height, Some("from"))?, 1)
            }
            None => (self.dummy_view.clone(), 0),
        };
        let to_view = self.upload_wallpaper(&to_pixels, width, height, Some("to"))?;

        let uniform = TransitionUniform {
            resolution: [width as f32, height as f32],
            origin: params.origin,
            angle: params.angle,
            progress: 0.0,
            effect: Self::effect_id(params.tran_type),
            has_from,
        };
        let uniform_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("transition_uniform"),
            size: std::mem::size_of::<TransitionUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&uniform_buf, 0, bytemuck::bytes_of(&uniform));

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("transition_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&to_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&from_view) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });

        Ok(RenderState { uniform_buf, bind_group, has_from })
    }

    /// Map the transition type to the shader's effect id.
    fn effect_id(tran_type: TransitionType) -> u32 {
        match tran_type {
            TransitionType::None => 0,
            TransitionType::Circle => 1,
            TransitionType::Wipe => 2,
        }
    }

    /// Rewrite the per-monitor uniform for the next frame of a transition.
    /// Cheap: 32 bytes through the staging path, ordered before the next
    /// submit, so the following render pass sees the new values.
    pub fn update_uniform(
        &self,
        state: &RenderState,
        params: &TransitionParams,
        progress: f32,
        width: u32,
        height: u32,
    ) {
        let uniform = TransitionUniform {
            resolution: [width as f32, height as f32],
            origin: params.origin,
            angle: params.angle,
            progress,
            effect: Self::effect_id(params.tran_type),
            has_from: state.has_from,
        };
        self.queue
            .write_buffer(&state.uniform_buf, 0, bytemuck::bytes_of(&uniform));
    }

    /// Upload one monitor-sized BGRA buffer as a sampleable texture.
    /// write_texture goes through a staging path, so bytes_per_row does not
    /// need to be a multiple of 256.
    fn upload_wallpaper(
        &self,
        pixels: &[u8],
        width: u32,
        height: u32,
        label: Option<&str>,
    ) -> anyhow::Result<wgpu::TextureView> {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label,
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        Ok(texture.create_view(&wgpu::TextureViewDescriptor::default()))
    }

    /// Draw one frame of the wallpaper in `state` onto `surf` and present it.
    /// `config` is the surface's configuration, used to recover from an
    /// outdated surface by reconfiguring it and skipping the frame.
    pub fn present_frame(
        &self,
        surf: &Surface,
        state: &RenderState,
        config: &wgpu::SurfaceConfiguration,
    ) -> anyhow::Result<()> {
        let frame = match surf.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                tracing::warn!("surface is suboptimal, consider reconfiguring");
                frame
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                surf.configure(&self.device, config);
                tracing::warn!("surface outdated, reconfigured; skipping frame");
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                tracing::debug!("surface busy or occluded; skipping frame");
                return Ok(());
            }
            other => anyhow::bail!("get_current_texture failed: {other:?}"),
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("render_encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.render_pipeline);
            pass.set_bind_group(0, &state.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(frame);
        // Pump wgpu's progress so completion callbacks run and dropped
        // resources are actually freed instead of accumulating.
        self.maintain();
        Ok(())
    }

    /// Pump wgpu's internal progress: run completion callbacks and free
    /// dropped resources. Non-blocking; called after every present and once
    /// per event loop iteration.
    pub fn maintain(&self) {
        let _ = self.device.poll(wgpu::PollType::Poll);
    }
}
