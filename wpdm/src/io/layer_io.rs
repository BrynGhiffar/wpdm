use std::{collections::VecDeque, ptr::NonNull};

use anyhow::Context;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState}, delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer, delegate_registry, delegate_seat, output::{OutputHandler, OutputState}, registry::{ProvidesRegistryState, RegistryState}, registry_handlers, seat::{Capability, SeatHandler, SeatState, keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers}, pointer::{PointerEvent, PointerEventKind, PointerHandler}}, shell::{WaylandSurface, wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure}}
};

use wayland_client::{
    Connection, EventQueue, Proxy, QueueHandle, globals::registry_queue_init, protocol::{wl_keyboard::WlKeyboard, wl_output::{Transform, WlOutput}, wl_pointer::WlPointer, wl_seat::WlSeat, wl_surface::WlSurface}
};
use wpdm_common::{TransitionType, WpdmMonitor, WpdmMonitors};

use crate::{
    decode::decode_bgra,
    io::{
        event::{StartTransitionReq, WpdmIoEvent, WpdmIoOutputEvent, WpdmIoRenderEvent, WpdmIoRenderRequest, WpdmOutputInfo},
        fluid::{SimState, Splat},
        gpu::{GpuContext, RenderState}
    }
};


#[derive(Debug)]
pub struct WpdmOutput {
    pub name: String,
    // The wgpu surface points into the wl_surface, so it must be dropped
    // before layer: fields drop in declaration order.
    pub surface: Option<wgpu::Surface<'static>>,
    pub surface_config: Option<wgpu::SurfaceConfiguration>,
    pub layer: LayerSurface,
    pub width: i32,
    pub height: i32,
    pub configured: bool,
    pub render: Option<RenderState>,
    pub sim: Option<SimState>
}

impl WpdmOutput {
    pub fn to_oi(&self) -> WpdmOutputInfo {
        WpdmOutputInfo { name: self.name.clone(), width: self.width, height: self.height }
    }
}

pub struct WpdmLayerIO {
    pub conn: Connection,
    pub registry_state: RegistryState,
    pub seat_state: SeatState,
    pub output_state: OutputState,
    pub layer_shell: LayerShell,
    pub compositor_state: CompositorState,
    pub qh: QueueHandle<Self>,
    pub outputs: Vec<WpdmOutput>,
    pub io_queue: VecDeque<WpdmIoEvent>,
    pub gpu_context: GpuContext,
    /// Last surface-local position per pointer, for splat deltas.
    pointer_prev: Vec<(WlPointer, (f64, f64))>
}

impl WpdmLayerIO {
    pub fn new() -> anyhow::Result<(Self, EventQueue<Self>)> {
        let conn = Connection::connect_to_env()?;
        let (globals, event_queue) = registry_queue_init::<Self>(&conn)?;
        let qh = event_queue.handle();

        let compositor_state = CompositorState::bind(&globals, &qh)?;

        let layer_shell = LayerShell::bind(&globals, &qh)?;

        let gpu_context = GpuContext::new()?;

        Ok((Self {
            conn,
            registry_state: RegistryState::new(&globals),
            seat_state: SeatState::new(&globals, &qh),
            output_state: OutputState::new(&globals, &qh),
            layer_shell,
            compositor_state,
            qh,
            outputs: vec![],
            io_queue: VecDeque::new(),
            gpu_context,
            pointer_prev: vec![]
        }, event_queue))
    }

    pub fn start_transition(&mut self, StartTransitionReq {
        oi,
        from,
        to,
        anim,
        sim
    }: StartTransitionReq) -> anyhow::Result<()> {
        let gpu = &self.gpu_context;
        let Some(idx) = self.outputs.iter().position(|out| out.name == oi.name) else {
            tracing::error!("Cannot start transition: output {} not found", oi.name);
            return Ok(());
        };

        let (pixels, width, height) = match decode_bgra(&to) {
            Ok(result) => result,
            Err(err) => {
                tracing::error!("Failed to decode wallpaper {to:?}: {err}");
                return Ok(());
            }
        };

        let output = self.outputs.get_mut(idx).unwrap();
        if !output.configured {
            tracing::error!("Cannot start transition on unconfigured output {}", oi.name);
            return Ok(());
        }
        let Some(surf) = output.surface.as_ref() else {
            tracing::error!("Cannot start transition: no wgpu surface for output {}", oi.name);
            return Ok(());
        };
        let Some(config) = output.surface_config.as_ref() else {
            tracing::error!("Cannot start transition: no surface config for output {}", oi.name);
            return Ok(());
        };

        if sim {
            // Fluid sim mode: the sim drives every presented frame and
            // transitions are hard cuts; the animated mask would need a
            // second wallpaper composite, so we decline it for now.
            if !matches!(anim.tran_type, TransitionType::None) {
                tracing::warn!("sim mode runs hard cuts; ignoring transition type for output {}", oi.name);
            }

            let sim_state = output.sim.get_or_insert_with(|| {
                tracing::info!("Initializing fluid sim on output {}", oi.name);
                SimState::new(gpu, (oi.width as u32, oi.height as u32))
            });
            if let Err(err) = sim_state.set_wallpaper(gpu, &pixels, width, height) {
                tracing::error!("Failed to upload wallpaper on output {}: {err}", oi.name);
                return Ok(());
            }

            // Arm the loop: the frame request must precede the commit, and
            // the present inside render_frame is the commit that arms it.
            let wls = output.layer.wl_surface();
            wls.frame(&self.qh, wls.clone());
            sim_state.wake();

            if let Err(err) = sim_state.render_frame(gpu, surf, config) {
                tracing::error!("Failed to render sim frame on output {}: {err}", oi.name);
                return Ok(());
            }
            tracing::info!("Sim wallpaper set on output {}", oi.name);
            return Ok(());
        }

        // Static path: tear the fluid sim down if it was running (the
        // fields and wallpaper texture free back once the pending loop
        // callback — if any — has been consumed).
        if output.sim.take().is_some() {
            tracing::info!("Stopping fluid sim on output {}", oi.name);
        }

        let state = match gpu.create_render_state(oi.width as u32, oi.height as u32, from.as_deref(), &to, &anim) {
            Ok(state) => state,
            Err(err) => {
                tracing::error!("Failed to create render state for output {}: {err}", oi.name);
                return Ok(());
            }
        };
        let animated = !matches!(anim.tran_type, TransitionType::None);

        // For animated transitions, request the first frame callback before
        // presenting: the present commits the surface and arms the callback.
        if animated {
            output.layer.wl_surface().frame(&self.qh, output.layer.wl_surface().clone());
        }

        if let Err(err) = gpu.present_frame(surf, &state, config) {
            tracing::error!("Failed to present frame for output {}: {err}", oi.name);
            return Ok(());
        }

        // Hard cuts have nothing left to animate: don't keep the render
        // state (two full-resolution textures) alive in the idle daemon.
        // The swapchain keeps showing the presented frame.
        if animated {
            self.outputs[idx].render = Some(state);
        }
        tracing::info!("Rendered wallpaper on output {}", oi.name);
        Ok(())
    }

    pub fn render(&mut self, WpdmIoRenderRequest {
        oi,
        params,
        progress,
        render_next
    }: WpdmIoRenderRequest) -> anyhow::Result<()> {
        let gpu = &self.gpu_context;
        let Some(idx) = self.outputs.iter().position(|out| out.name == oi.name) else {
            tracing::error!("Cannot render: output {} not found", oi.name);
            return Ok(());
        };
        let output = &self.outputs[idx];
        if !output.configured {
            tracing::error!("output needs to be configured before rendered");
            return Ok(());
        }
        let Some(surf) = output.surface.as_ref() else {
            return Ok(());
        };
        let Some(config) = output.surface_config.as_ref() else {
            return Ok(());
        };
        let Some(state) = output.render.as_ref() else {
            tracing::error!("Cannot render: no render state for output {}", oi.name);
            return Ok(());
        };

        gpu.update_uniform(state, &params, progress, oi.width as u32, oi.height as u32);
        // Request the next callback before presenting: the present commits
        // the surface and arms the callback.
        if render_next {
            output.layer.wl_surface().frame(&self.qh, output.layer.wl_surface().clone());
        }
        if let Err(err) = gpu.present_frame(surf, state, config) {
            tracing::error!("Failed to present frame for output {}: {err}", oi.name);
            return Ok(());
        }
        if progress >= 1.0 {
            // Transition complete: the swapchain keeps showing the final
            // frame, so drop the render state (two full-resolution
            // textures per monitor) instead of holding it in the idle
            // daemon. The next wallpaper change re-uploads anyway.
            self.outputs[idx].render = None;
        }
        Ok(())
    }

    pub fn monitor_query(&self) -> WpdmMonitors {
        let monitors = self.outputs
            .iter()
            .map(|out| WpdmMonitor {
                name: out.name.to_string(),
                width: out.width,
                height: out.height
            }).collect();
        WpdmMonitors { monitors }
    }

    pub fn get_output_by_name(&self, name: &str) -> Option<&WpdmOutput> {
        self.outputs
            .iter()
            .find(|out| out.name == name)
    }

    fn extract_output_info(&self, output: &WlOutput) -> anyhow::Result<WpdmOutputInfo> {
        let output_info = self
            .output_state
            .info(output)
            .context("Failed to get output info")?;
        let monitor_name = output_info
            .name
            .context("Failed to get monitor_name")?
            .clone();
        let (width, height) = output_info
            .logical_size
            .context("Failed to get monitor width and height")?;

        Ok(WpdmOutputInfo {
            name: monitor_name,
            width,
            height,
        })
    }
}

impl CompositorHandler for WpdmLayerIO {
    fn frame(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        surface: &WlSurface,
        _time: u32,
    ) {
        let Some(idx) = self.outputs.iter().position(|out| out.layer.wl_surface() == surface) else {
            tracing::error!("LOST_FRAME: Cannot render frame on uninitialized output");
            return;
        };
        let output = &mut self.outputs[idx];

        // Fluid sim outputs render inline: splats + pass chain + composite,
        // all in one encoder, then present. The previous frame requested
        // the callback; when the sim settles it stops requesting and the
        // loop ends here.
        if let Some(sim) = output.sim.as_mut() {
            if sim.active() {
                let wls = output.layer.wl_surface();
                wls.frame(qh, wls.clone());
            }
            let Some(surf) = output.surface.as_ref() else {
                return;
            };
            let Some(config) = output.surface_config.as_ref() else {
                return;
            };
            if let Err(err) = sim.render_frame(&self.gpu_context, surf, config) {
                tracing::error!("sim frame failed on {}: {err}", output.name);
            }
            return;
        }

        self.io_queue
            .push_back(WpdmIoEvent::Render(WpdmIoRenderEvent { oi: output.to_oi() }));
    }

    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _new_factor: i32,
    ) {
        // Not needed for this example.
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _new_transform: Transform,
    ) {
        // Not needed for this example.
    }
    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _output: &WlOutput,
    ) {
        // Not needed for this example.
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _output: &WlOutput,
    ) {
        // Not needed for this example.
    }
}

impl OutputHandler for WpdmLayerIO {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        output: WlOutput,
    ) {
        let output_info = self.extract_output_info(&output);
        let Ok(output_info) = output_info else {
            tracing::error!("Failed to extract output info");
            return;
        };
        tracing::info!("new output event: {:?}", &output_info);
        let surface = self.compositor_state.create_surface(qh);
        let layer = self.layer_shell.create_layer_surface(
            qh,
            surface,
            Layer::Background,
            Some("wpdm"),
            Some(&output),
        );
        layer.set_anchor(Anchor::BOTTOM);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_size(output_info.width as u32, output_info.height as u32);
        layer.commit();

        let raw_display_handle = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
            NonNull::new(self.conn.backend().display_ptr() as *mut _).unwrap(),
        ));

        let raw_window_handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(
            NonNull::new(layer.wl_surface().id().as_ptr() as *mut _).unwrap(),
        ));

        let surface = unsafe {
            self.gpu_context.instance
                .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                    raw_display_handle: Some(raw_display_handle),
                    raw_window_handle,
                })
                .unwrap()
        };

        let output = WpdmOutput {
            name: output_info.name.clone(),
            width: output_info.width,
            height: output_info.height,
            layer,
            configured: false,
            surface: Some(surface),
            surface_config: None,
            render: None,
            sim: None
        };

        let oi = output.to_oi();
        self.outputs.push(output);
        self.io_queue.push_back(WpdmIoEvent::NewOutput(WpdmIoOutputEvent { oi }))
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: WlOutput,
    ) {
        tracing::info!("UPDATE OUTPUT EVENT");
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        output: WlOutput,
    ) {
        let output_info = self.extract_output_info(&output);
        let Ok(oi) = output_info else {
            tracing::error!("Failed to extract output info");
            return;
        };
        self.outputs.retain(|output | output.name != oi.name);
        tracing::info!("Destroyed Output Event: {:?}", oi);
        self.io_queue.push_back(WpdmIoEvent::DestroyOutput(WpdmIoOutputEvent { oi }));
    }
}

impl LayerShellHandler for WpdmLayerIO {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        tracing::info!("LAYER CLOSED");
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        _configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let Some(output) = self.outputs
            .iter_mut()
            .find(|out| out.layer.wl_surface() == layer.wl_surface()) else {
            return;
        };
        if output.configured {
            return;
        }
        output.configured = true;
        let oi = output.to_oi();
        if let Some(surf) = output.surface.as_ref() {
            let surface_config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: wgpu::TextureFormat::Bgra8Unorm,
                view_formats: vec![wgpu::TextureFormat::Bgra8Unorm],
                alpha_mode: wgpu::CompositeAlphaMode::Opaque,
                width: oi.width as u32,
                height: oi.height as u32,
                // Fifo + latency 1: one full-resolution buffer less than
                // Mailbox, worth 30 MB+ per 4K output. Frame pacing comes
                // from wl_surface.frame callbacks anyway.
                desired_maximum_frame_latency: 1,
                present_mode: wgpu::PresentMode::Fifo,
                color_space: wgpu::SurfaceColorSpace::Auto
            };
            // Keep the config around so the render path can recover from an
            // outdated surface by reconfiguring it.
            output.surface_config = Some(surface_config.clone());
            surf.configure(&self.gpu_context.device, &surface_config);
        };

        self.io_queue.push_back(WpdmIoEvent::ConfigureOutput(WpdmIoOutputEvent { oi }));
    }
}

impl SeatHandler for WpdmLayerIO {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: WlSeat,
        capability: Capability,
    ) {
        // PROBE (fluid sim, phase 0): bind the pointer so the compositor can
        // deliver pointer focus to our surfaces. Whether background layer
        // surfaces actually receive it is what this probe is for; the
        // validated answer feeds the fluid sim (phase 1) below.
        if matches!(capability, Capability::Pointer) {
            match self.seat_state.get_pointer(qh, &seat) {
                Ok(_) => tracing::info!("PROBE pointer: bound pointer"),
                Err(err) => tracing::error!("PROBE pointer: failed to bind pointer: {err}"),
            }
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: WlSeat,
        capability: Capability,
    ) {
        // PROBE (fluid sim, phase 0): re-bind on capability reappearing.
        if matches!(capability, Capability::Pointer) {
            let _ = self.seat_state.get_pointer(qh, &seat);
        }
    }

    fn remove_seat(&mut self, _conn: &Connection, _: &QueueHandle<Self>, _: WlSeat) {}
}

impl KeyboardHandler for WpdmLayerIO {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _: &WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _: &WlSurface,
        _: u32,
    ) {
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }

    fn repeat_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &WlKeyboard,
        _serial: u32,
        _event: KeyEvent,
    ) {
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _: u32,
        _: Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
    }
}

impl PointerHandler for WpdmLayerIO {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        pointer: &WlPointer,
        events: &[PointerEvent],
    ) {
        // Fluid sim: pointer motion becomes velocity + offset splats on the
        // sim-enabled output the pointer is over. The probe logging from
        // phase 0 is kept at debug level.
        for event in events {
            let Some(idx) = self.outputs.iter().position(|out| out.layer.wl_surface() == &event.surface) else {
                continue;
            };
            let output = &self.outputs[idx];
            match &event.kind {
                PointerEventKind::Enter { .. } => {
                    tracing::info!("PROBE pointer: enter on {}", output.name);
                    self.pointer_prev.retain(|(p, _)| p != pointer);
                    self.pointer_prev.push((pointer.clone(), event.position));
                }
                PointerEventKind::Leave { .. } => {
                    tracing::info!("PROBE pointer: leave on {}", output.name);
                    self.pointer_prev.retain(|(p, _)| p != pointer);
                }
                PointerEventKind::Motion { .. } => {
                    tracing::debug!(
                        "PROBE pointer: motion on {} at ({:.0}, {:.0})",
                        output.name,
                        event.position.0,
                        event.position.1
                    );
                    let position = event.position;
                    let delta = match self.pointer_prev.iter_mut().find(|(p, _)| p == pointer) {
                        Some(prev) => {
                            let d = (position.0 - prev.1 .0, position.1 - prev.1 .1);
                            prev.1 = position;
                            Some(d)
                        }
                        None => {
                            self.pointer_prev.push((pointer.clone(), position));
                            None
                        }
                    };
                    let Some((dx, dy)) = delta else { continue };
                    if (dx, dy) == (0.0, 0.0) {
                        continue;
                    }
                    // Local copies so the immutable borrow of `output` ends
                    // before we take the mutable borrow of the sim.
                    let wls = output.layer.wl_surface().clone();
                    let (w, h) = (output.width, output.height);
                    if let Some(sim) = self.outputs[idx].sim.as_mut() {
                        if sim.is_sleeping() {
                            // Wake the sim: request the callback + an empty
                            // commit to arm it; render_frame does the presents.
                            wls.frame(qh, wls.clone());
                            wls.commit();
                            sim.wake();
                        }
                        let uv = [
                            (position.0 as f32 / w as f32).clamp(0.0, 1.0),
                            (position.1 as f32 / h as f32).clamp(0.0, 1.0),
                        ];
                        sim.push_splat(Splat {
                            pos: uv,
                            dxy: [dx as f32, dy as f32],
                        });
                    }
                }
                other => {
                    tracing::info!("PROBE pointer: {other:?} on {}", output.name);
                }
            }
        }
    }
}

impl ProvidesRegistryState for WpdmLayerIO {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(WpdmLayerIO);
delegate_output!(WpdmLayerIO);
delegate_seat!(WpdmLayerIO);
delegate_keyboard!(WpdmLayerIO);
delegate_pointer!(WpdmLayerIO);
delegate_layer!(WpdmLayerIO);
delegate_registry!(WpdmLayerIO);
