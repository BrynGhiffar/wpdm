extern crate libc;

use std::sync::{Arc, RwLock};

use anyhow::Context;
use memmap2::Mmap;
use smithay_client_toolkit::{
    compositor::CompositorState,
    output::OutputState,
    registry::RegistryState,
    seat::SeatState,
    shell::{
        WaylandSurface,
        wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerSurface},
    },
    shm::{
        Shm,
        slot::{Buffer, SlotPool},
    },
};
use wayland_client::{
    Connection, EventQueue, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_output, wl_shm, wl_surface::WlSurface},
};

use std::sync::mpsc::Receiver;

use crate::{cmd::RenderCmd, transitions::TransitionAnim};

#[allow(unused)]
use crate::loader::load_argb_buffer;
use crate::loader::mmap_buffer;

#[derive(Clone, Debug)]
pub struct MonitorMeta {
    pub name: String,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone)]
pub struct Monitor {
    pub name: String,
    pub layer: LayerSurface,
    pub width: i32,
    pub height: i32,
    pub configured: bool,
}

pub struct Transition {
    monitor: String,
    frame: u32,
    from_buffer: Mmap,
    to_buffer: Mmap,
    transition: TransitionAnim,
}

pub struct TransitionManager {
    pub transitions: Vec<Transition>,
}

impl TransitionManager {
    fn new() -> Self {
        Self {
            transitions: vec![],
        }
    }

    fn render_transition(&mut self, monitor: &str, buffer: &mut [u8]) -> Option<()> {
        let tr_idx = self
            .transitions
            .iter()
            .position(|tr| tr.monitor.eq(monitor))?;

        // No monitor is left behind!
        let frame = self.transitions.iter().map(|tr| tr.frame).min()?;
        let tr = self.transitions.get_mut(tr_idx)?;

        let ret = tr
            .transition
            .render(frame, &tr.from_buffer, &tr.to_buffer, buffer);
        if !ret {
            tr.frame = frame + 1;
        } else {
            let Transition {
                from_buffer,
                to_buffer,
                ..
            } = self.transitions.remove(tr_idx);
            drop(from_buffer);
            drop(to_buffer);
        }
        Some(())
    }

    fn has_transitions(&self) -> bool {
        !self.transitions.is_empty()
    }
}

pub type SharedMonitorMeta = Arc<RwLock<Vec<MonitorMeta>>>;
pub struct WallpaperLayer {
    pub registry_state: RegistryState,
    pub seat_state: SeatState,
    pub output_state: OutputState,
    pub event_queue: Option<EventQueue<Self>>,
    pub layer_shell: LayerShell,
    pub compositor_state: CompositorState,
    pub pool: Option<SlotPool>,
    pub shm: Shm,

    cons: Receiver<RenderCmd>,
    monitor_meta: SharedMonitorMeta,
    monitors: Vec<Monitor>,
    transition_manager: Option<TransitionManager>,
}

impl WallpaperLayer {
    pub fn new(cons: Receiver<RenderCmd>) -> anyhow::Result<Self> {
        let conn = Connection::connect_to_env()?;
        let (globals, event_queue) = registry_queue_init::<Self>(&conn)?;
        let qh = event_queue.handle();

        let compositor_state = CompositorState::bind(&globals, &qh)?;

        let shm = Shm::bind(&globals, &qh)?;

        let pool = None;
        let layer_shell = LayerShell::bind(&globals, &qh)?;
        let monitors = vec![];

        Ok(Self {
            registry_state: RegistryState::new(&globals),
            seat_state: SeatState::new(&globals, &qh),
            output_state: OutputState::new(&globals, &qh),
            event_queue: Some(event_queue),
            layer_shell,
            compositor_state,
            pool,
            shm,

            cons,
            monitor_meta: Arc::new(RwLock::new(vec![])),
            monitors,
            transition_manager: Some(TransitionManager::new()),
        })
    }

    pub fn setup_monitor(
        &mut self,
        qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) -> anyhow::Result<()> {
        tracing::info!("New Monitor!");

        // This should be in a method.
        let monitor_meta = self.create_monitor_meta(&output)?;
        tracing::info!("Monitor Info: {:?}", monitor_meta);

        let layer = self.create_layer_shell(qh, &output, &monitor_meta);

        let monitor = Monitor {
            name: monitor_meta.name.clone(),
            width: monitor_meta.width,
            height: monitor_meta.height,
            layer,
            configured: false,
        };
        self.monitors.push(monitor);
        let mut mons = self.monitor_meta.write().unwrap();
        mons.push(monitor_meta);
        Ok(())
    }

    pub fn render(
        &mut self,
        qh: &QueueHandle<Self>,
        surface: &WlSurface,
        configure: bool,
    ) -> anyhow::Result<()> {
        // 1. Poll for any new frames
        // 2. Render new frame
        // tracing::info!("RENDERING FRAME");
        let monitor = self
            .get_monitor(surface, configure)
            .context("Monitor not found")?;
        if !monitor.configured {
            return Ok(());
        }

        let mut transition_manager = self
            .transition_manager
            .take()
            .context("Missing transition manager")?;

        let (buffer, canvas) = self.create_buffer(&monitor)?;
        transition_manager.render_transition(&monitor.name, canvas);

        let has_transitions = transition_manager.has_transitions();
        self.transition_manager.replace(transition_manager);
        self.flush_buffer(&buffer, &monitor)?;

        self.request_render(qh, &monitor);

        if !has_transitions {
            let result = unsafe { libc::malloc_trim(0) };
            if result == 1 {
                tracing::info!("Memory was released back to the system.");
            } else {
                tracing::info!(
                    "No memory could be released, or the function is not available on this platform."
                );
            }
            self.pool.take();
            self.wait_for_commands(true);
        } else {
            self.wait_for_commands(false);
        }

        Ok(())
    }

    fn wait_for_commands(&mut self, blocking: bool) {
        let recv = || {
            if blocking {
                self.cons.recv().context("Recv err")
            } else {
                self.cons.try_recv().context("Try Recv Err")
            }
        };
        let Ok(RenderCmd {
            monitor,
            src_argb_buff_path,
            dest_argb_buff_path,
            tr_type,
            tr_origin,
            angle,
        }) = recv()
        else {
            return;
        };

        let Some((width, height)) = self.get_monitor_size(&monitor) else {
            return;
        };

        // Only expected to loop once, since message from upstream, must be one message,
        // per monitor size
        let Ok(from_buffer) = mmap_buffer(src_argb_buff_path.clone()) else {
            return;
        };
        let Ok(to_buffer) = mmap_buffer(dest_argb_buff_path.clone()) else {
            return;
        };
        let expected_buffer_len = (width * height * 4) as usize;
        if from_buffer.len() != expected_buffer_len {
            tracing::error!(
                "Failed to create transition, since from buffer len is unexpected size: {}",
                from_buffer.len()
            );
            return;
        }

        if to_buffer.len() != expected_buffer_len {
            tracing::error!(
                "Failed to create transition, since to buffer len is unexpected size: {}",
                to_buffer.len()
            );
            return;
        }

        let transition = match tr_type {
            wpdm_common::TransitionType::Wipe => {
                TransitionAnim::wipe(width, height, angle, tr_origin)
            }
            wpdm_common::TransitionType::Circle => TransitionAnim::circle(width, height, tr_origin),
            wpdm_common::TransitionType::None => TransitionAnim::none(width, height),
        };

        let tr = Transition {
            frame: 0,
            monitor,
            transition,
            from_buffer,
            to_buffer,
        };

        if let Some(trm) = self.transition_manager.as_mut() {
            trm.transitions.push(tr);
        }
    }

    fn get_monitor_size(&self, monitor: &str) -> Option<(u32, u32)> {
        let read_shared = self.monitor_meta.read().unwrap();
        let meta = read_shared.iter().find(|meta| meta.name == monitor)?;
        Some((meta.width as u32, meta.height as u32))
    }

    fn get_monitor(&mut self, surface: &WlSurface, configure: bool) -> Option<Monitor> {
        if configure {
            let monitor = self
                .monitors
                .iter_mut()
                .find(|m| m.layer.wl_surface() == surface)?;
            monitor.configured = configure;
            return Some(monitor.clone());
        }
        let monitor = self
            .monitors
            .iter()
            .find(|m| m.layer.wl_surface() == surface)?;
        Some(monitor.clone())
    }

    fn request_render(&self, qh: &QueueHandle<Self>, monitor: &Monitor) {
        monitor
            .layer
            .wl_surface()
            .frame(qh, monitor.layer.wl_surface().clone());
        monitor.layer.commit();
    }

    fn flush_buffer(&self, buffer: &Buffer, monitor: &Monitor) -> anyhow::Result<()> {
        buffer.attach_to(monitor.layer.wl_surface())?;
        monitor
            .layer
            .wl_surface()
            .damage_buffer(0, 0, monitor.width, monitor.height);
        Ok(())
    }

    fn create_buffer(&mut self, monitor: &Monitor) -> anyhow::Result<(Buffer, &mut [u8])> {
        let pool = if self.pool.is_some() {
            self.pool.as_mut()
        } else {
            self.pool = Some(SlotPool::new(1, &self.shm)?);
            self.pool.as_mut()
        };

        let (buffer, canvas) = pool.unwrap().create_buffer(
            monitor.width,
            monitor.height,
            monitor.width * 4,
            wl_shm::Format::Argb8888,
        )?;

        Ok((buffer, canvas))
    }

    pub fn get_monitor_meta(&self) -> SharedMonitorMeta {
        self.monitor_meta.clone()
    }

    fn create_monitor_meta(&self, output: &wl_output::WlOutput) -> anyhow::Result<MonitorMeta> {
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

        Ok(MonitorMeta {
            name: monitor_name,
            width,
            height,
        })
    }

    fn create_layer_shell(
        &self,
        qh: &QueueHandle<Self>,
        output: &wl_output::WlOutput,
        monitor_meta: &MonitorMeta,
    ) -> LayerSurface {
        let surface = self.compositor_state.create_surface(qh);
        let layer = self.layer_shell.create_layer_surface(
            qh,
            surface,
            Layer::Background,
            Some("background_layer"),
            Some(output),
        );
        layer.set_anchor(Anchor::BOTTOM);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_size(monitor_meta.width as u32, monitor_meta.height as u32);
        layer.commit();
        layer
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        let Some(mut evt_queue) = self.event_queue.take() else {
            return Ok(());
        };
        tracing::info!("Running Layer");

        evt_queue.roundtrip(self)?;

        loop {
            evt_queue.blocking_dispatch(self)?;
        }
    }
}
