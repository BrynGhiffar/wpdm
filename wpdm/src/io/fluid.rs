//! Per-output fluid simulation state — a port of the sim core of
//! Paveldogreat's WebGL-Fluid-Simulation (MIT, © Pavel Dobryakov) to wgpu,
//! driving an offset-distortion field (design B) instead of dye colors:
//! the final composite resamples the wallpaper through the offset field, so
//! the field heals to the pristine wallpaper as it dissipates.

use std::{collections::VecDeque, num::NonZero, time::{Duration, Instant}};

use wgpu::{Surface, SurfaceConfiguration};

use crate::io::gpu::GpuContext;

#[allow(dead_code)]

pub const SIMU_SIZE: u64 = 64;
const UNIFORM_SLICE: u64 = 256;
const UNIFORM_SLICES: u64 = 64;

// Reference config ports; OFFSET_FORCE is the design-B addition.
const VELOCITY_DISSIPATION: f32 = 0.2;
const OFFSET_DISSIPATION: f32 = 1.0;
const PRESSURE_BIAS: f32 = 0.8;
const PRESSURE_ITERATIONS: u32 = 20;
const CURL_STRENGTH: f32 = 30.0;
const SPLAT_FORCE: f32 = 15.0;
const SPLAT_RADIUS: f32 = 0.0025; // config.SPLAT_RADIUS (0.25) / 100
const OFFSET_FORCE: f32 = 1.0;
const SPLATS_PER_FRAME: usize = 12;
const PENDING_CAP: usize = 96;
const SIM_DIVISOR: u32 = 8;
const OFFSET_DIVISOR: u32 = 4;

pub const SETTLE_TIME: Duration = Duration::from_secs(10);

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SimU {
    pub texel: [f32; 2],
    pub resolution: [f32; 2],
    pub dt: f32,
    pub dissipation: f32,
    pub curl: f32,
    pub pressure: f32,
    pub splat_pos: [f32; 2],
    pub splat_duv: [f32; 2],
    pub splat_aspect: f32,
    pub splat_radius: f32,
    pub pad: [f32; 2],
}

#[derive(Debug)]
struct Ping {
    a: wgpu::TextureView,
    b: wgpu::TextureView,
}

impl Ping {
    fn front(&self) -> &wgpu::TextureView { &self.a }
    fn back(&self) -> &wgpu::TextureView { &self.b }
    fn flip(&mut self) { std::mem::swap(&mut self.a, &mut self.b) }
}

/// A cursor-driven splat: position + pointer delta in uv space.
#[derive(Debug, Clone, Copy)]
pub struct Splat {
    pub pos: [f32; 2],
    pub dxy: [f32; 2],
}

#[derive(Debug)]
pub struct SimState {
    screen: [f32; 2],
    sim: (u32, u32),
    uniforms: wgpu::Buffer,
    vel: Ping,
    pressure: Ping,
    curl: wgpu::TextureView,
    divergence: wgpu::TextureView,
    off: Ping,
    wallpaper: wgpu::TextureView,
    last_frame: Option<Instant>,
    settle_at: Instant,
    sleeping: bool,
    pending: VecDeque<Splat>,
}

#[allow(clippy::too_many_arguments)]
fn run_pass(
    gpu: &GpuContext,
    encoder: &mut wgpu::CommandEncoder,
    uniforms: &wgpu::Buffer,
    cursor: &mut u32,
    pipeline: &wgpu::RenderPipeline,
    target: &wgpu::TextureView,
    ins: [&wgpu::TextureView; 2],
    u: SimU,
) {
    let slice = *cursor;
    *cursor += 1;
    let uniform_offset = slice as u64 * UNIFORM_SLICE;
    gpu.queue.write_buffer(uniforms, uniform_offset, bytemuck::bytes_of(&u));

    // The bind group is rebuilt per pass (the ping-pong sources alternate);
    // wgpu keeps them alive until the in-flight submit completes.
    let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("sim_pass_bind"),
        layout: &gpu.sim_bind_group_layout,
        entries: &[
            // Explicit slice size: the whole-buffer binding would leave no
            // room for dynamic offsets (offset + size must fit the buffer).
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: uniforms,
                    offset: 0,
                    size: Some(NonZero::new(SIMU_SIZE).unwrap()),
                }),
            },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&gpu.sampler) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(ins[0]) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(ins[1]) },
        ],
    });

    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("sim_pass_render"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
            depth_slice: None,
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, &bind, &[uniform_offset as u32]);
    pass.draw(0..3, 0..1);
}

fn clear_view(encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
    // Fullscreen clear with no draws: the pass load op does the work.
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("sim_field_clear"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
            depth_slice: None,
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
}

fn field_view(
    gpu: &GpuContext,
    width: u32,
    height: u32,
    label: &str,
    format: wgpu::TextureFormat,
) -> wgpu::TextureView {
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Queue a splat invocation.
#[allow(clippy::too_many_arguments)]
fn splat_pass(
    gpu: &GpuContext,
    encoder: &mut wgpu::CommandEncoder,
    uniforms: &wgpu::Buffer,
    cursor: &mut u32,
    pipeline: &wgpu::RenderPipeline,
    field: &mut Ping,
    splat: &Splat,
    strength: f32,
    radius: f32,
    screen: [f32; 2],
) {
    let texel = field.size();
    let u = SimU {
        texel: [1.0 / texel.0 as f32, 1.0 / texel.1 as f32],
        resolution: screen,
        dt: 0.0,
        dissipation: 0.0,
        curl: 0.0,
        pressure: 0.0,
        splat_pos: splat.pos,
        splat_duv: [splat.dxy[0] * strength, splat.dxy[1] * strength],
        splat_aspect: screen[0] / screen[1],
        splat_radius: radius,
        pad: [0.0, 0.0],
    };
    run_pass(gpu, encoder, uniforms, cursor, pipeline, field.back(), [field.front(), &gpu.dummy_view], u);
    field.flip();
}

impl Ping {
    pub fn size(&self) -> (u32, u32) {
        (self.a.texture().width(), self.a.texture().height())
    }
}

impl SimState {
    pub fn new(gpu: &GpuContext, screen: (u32, u32)) -> Self {
        let sim = ((screen.0 / SIM_DIVISOR).max(32), (screen.1 / SIM_DIVISOR).max(32));
        let off_dim = ((screen.0 / OFFSET_DIVISOR).max(64), (screen.1 / OFFSET_DIVISOR).max(64));
        let uniforms = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sim_uniforms"),
            size: UNIFORM_SLICE * UNIFORM_SLICES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Sub-rgba formats: velocity/offsets live in xy (rg), pressure/
        // curl/divergence in x (r) — half and a quarter the rgba16f size.
        Self {
            screen: [screen.0 as f32, screen.1 as f32],
            sim,
            uniforms,
            vel: Ping { a: field_view(gpu, sim.0, sim.1, "sim_vel_a", wgpu::TextureFormat::Rg16Float), b: field_view(gpu, sim.0, sim.1, "sim_vel_b", wgpu::TextureFormat::Rg16Float) },
            pressure: Ping { a: field_view(gpu, sim.0, sim.1, "sim_pressure_a", wgpu::TextureFormat::R16Float), b: field_view(gpu, sim.0, sim.1, "sim_pressure_b", wgpu::TextureFormat::R16Float) },
            curl: field_view(gpu, sim.0, sim.1, "sim_curl", wgpu::TextureFormat::R16Float),
            divergence: field_view(gpu, sim.0, sim.1, "sim_divergence", wgpu::TextureFormat::R16Float),
            off: Ping { a: field_view(gpu, off_dim.0, off_dim.1, "sim_offset_a", wgpu::TextureFormat::Rg16Float), b: field_view(gpu, off_dim.0, off_dim.1, "sim_offset_b", wgpu::TextureFormat::Rg16Float) },
            wallpaper: gpu.dummy_view.clone(),
            last_frame: None,
            settle_at: Instant::now() + SETTLE_TIME,
            sleeping: true,
            pending: VecDeque::new(),
        }
    }

    /// Decode and replace the composited wallpaper texture.
    pub fn set_wallpaper(&mut self, gpu: &GpuContext, pixels: &[u8], width: u32, height: u32) -> anyhow::Result<()> {
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sim_wallpaper"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        self.wallpaper = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Ok(())
    }

    pub fn is_sleeping(&self) -> bool { self.sleeping }

    pub fn active(&self) -> bool { Instant::now() < self.settle_at }

    pub fn wake(&mut self) {
        self.sleeping = false;
        self.settle_at = Instant::now() + SETTLE_TIME;
    }

    pub fn push_splat(&mut self, splat: Splat) {
        if self.pending.len() >= PENDING_CAP {
            self.pending.pop_front();
        }
        self.pending.push_back(splat);
        self.settle_at = Instant::now() + SETTLE_TIME;
    }

    /// Run one sim frame: splats, the pass chain, composite into the
    /// swapchain. Returns whether the loop should keep running — the caller
    /// must have requested the frame callback before calling this (the
    /// present below is what arms it).
    pub fn render_frame(
        &mut self,
        gpu: &GpuContext,
        surf: &Surface,
        config: &SurfaceConfiguration,
    ) -> anyhow::Result<()> {
        let now = Instant::now();
        let dt = self
            .last_frame
            .map_or(1.0 / 60.0, |last| (now - last).as_secs_f32())
            .min(1.0 / 60.0);
        self.last_frame = Some(now);

        let frame = match surf.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                tracing::warn!("sim surface suboptimal, consider reconfiguring");
                frame
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                surf.configure(&gpu.device, config);
                tracing::warn!("sim surface outdated, reconfigured; skipping frame");
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            other => anyhow::bail!("sim acquire failed: {other:?}"),
        };
        let target = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("sim_encoder"),
        });

        if now >= self.settle_at {
            // Settled: near-zero offsets are not exactly zero, so clear every
            // field and present the pristine composite, then stop the loop.
            // A pointer splat wakes the sim again.
            let views: [&wgpu::TextureView; 8] = [
                &self.vel.a, &self.vel.b,
                &self.pressure.a, &self.pressure.b,
                &self.curl, &self.divergence,
                &self.off.a, &self.off.b,
            ];
            for view in views {
                clear_view(&mut encoder, view);
            }
            run_pass(gpu, &mut encoder, &self.uniforms, &mut 0, &gpu.sim_pipelines.composite, &target, [self.off.front(), &self.wallpaper], self.base_uniform(0.0));
            gpu.queue.submit(Some(encoder.finish()));
            gpu.queue.present(frame);
            self.sleeping = true;
            return Ok(());
        }

        let mut cursor: u32 = 0;

        // splats: velocity + offset injection per pending pointer event
        for _ in 0..SPLATS_PER_FRAME {
            let Some(splat) = self.pending.pop_front() else { break };
            splat_pass(gpu, &mut encoder, &self.uniforms, &mut cursor, &gpu.sim_pipelines.splat, &mut self.vel, &splat, SPLAT_FORCE, SPLAT_RADIUS, self.screen);
            splat_pass(gpu, &mut encoder, &self.uniforms, &mut cursor, &gpu.sim_pipelines.splat, &mut self.off, &splat, OFFSET_FORCE, SPLAT_RADIUS, self.screen);
        }

        let u = self.base_uniform(dt);
        run_pass(gpu, &mut encoder, &self.uniforms, &mut cursor, &gpu.sim_pipelines.curl, &self.curl, [self.vel.front(), &gpu.dummy_view], u);
        run_pass(gpu, &mut encoder, &self.uniforms, &mut cursor, &gpu.sim_pipelines.vorticity, self.vel.back(), [self.vel.front(), &self.curl], u);
        self.vel.flip();
        run_pass(gpu, &mut encoder, &self.uniforms, &mut cursor, &gpu.sim_pipelines.divergence, &self.divergence, [self.vel.front(), &gpu.dummy_view], u);
        run_pass(gpu, &mut encoder, &self.uniforms, &mut cursor, &gpu.sim_pipelines.bias, self.pressure.back(), [self.pressure.front(), &gpu.dummy_view], u);
        self.pressure.flip();
        for _ in 0..PRESSURE_ITERATIONS {
            run_pass(gpu, &mut encoder, &self.uniforms, &mut cursor, &gpu.sim_pipelines.jacobi, self.pressure.back(), [self.pressure.front(), &self.divergence], u);
            self.pressure.flip();
        }
        run_pass(gpu, &mut encoder, &self.uniforms, &mut cursor, &gpu.sim_pipelines.gradient, self.vel.back(), [self.vel.front(), self.pressure.front()], u);
        self.vel.flip();
        let u_vel = SimU { dissipation: VELOCITY_DISSIPATION, ..u };
        run_pass(gpu, &mut encoder, &self.uniforms, &mut cursor, &gpu.sim_pipelines.advect, self.vel.back(), [self.vel.front(), self.vel.front()], u_vel);
        self.vel.flip();
        let u_off = SimU { dissipation: OFFSET_DISSIPATION, ..u };
        run_pass(gpu, &mut encoder, &self.uniforms, &mut cursor, &gpu.sim_pipelines.advect, self.off.back(), [self.vel.front(), self.off.front()], u_off);
        self.off.flip();
        run_pass(gpu, &mut encoder, &self.uniforms, &mut cursor, &gpu.sim_pipelines.composite, &target, [self.off.front(), &self.wallpaper], self.base_uniform(0.0));

        gpu.queue.submit(Some(encoder.finish()));
        gpu.queue.present(frame);
        gpu.maintain();
        Ok(())
    }

    fn base_uniform(&self, dt: f32) -> SimU {
        SimU {
            texel: [1.0 / self.sim.0 as f32, 1.0 / self.sim.1 as f32],
            resolution: self.screen,
            dt,
            dissipation: 0.0,
            curl: CURL_STRENGTH,
            pressure: PRESSURE_BIAS,
            splat_pos: [0.0, 0.0],
            splat_duv: [0.0, 0.0],
            splat_aspect: self.screen[0] / self.screen[1],
            splat_radius: SPLAT_RADIUS,
            pad: [0.0, 0.0],
        }
    }
}
