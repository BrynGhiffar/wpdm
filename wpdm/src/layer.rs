use anyhow::Context;
use smithay_client_toolkit::{compositor::{CompositorHandler, CompositorState}, delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer, delegate_registry, delegate_seat, delegate_shm, output::{OutputHandler, OutputState}, registry::{ProvidesRegistryState, RegistryState}, registry_handlers, seat::{keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers}, pointer::{PointerEvent, PointerHandler}, Capability, SeatHandler, SeatState}, shell::{wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure}, WaylandSurface}, shm::{slot::SlotPool, Shm, ShmHandler}};
use wayland_client::{globals::registry_queue_init, protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface::{self, WlSurface}}, Connection, EventQueue, QueueHandle};

use crate::image_transition::ImageTransition;

#[derive(Clone)]
pub struct Monitor {
    pub layer: LayerSurface,
    pub width: i32,
    pub height: i32,
    pub configured: bool
}

pub struct WallpaperLayer {
    transition: ImageTransition,
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    event_queue: Option<EventQueue<Self>>,
    layer_shell: LayerShell,
    compositor_state: CompositorState,
    pool: SlotPool,
    shm: Shm,
    monitors: Vec<Monitor>,
}

impl WallpaperLayer {
    pub fn new(args: &[String]) -> anyhow::Result<Self> {

        let transition = ImageTransition::new(args);
        let conn = Connection::connect_to_env()?;
        let (globals, event_queue) = registry_queue_init::<WallpaperLayer>(&conn)?;
        let qh = event_queue.handle();

        let compositor_state = CompositorState::bind(&globals, &qh)?;

        let shm = Shm::bind(&globals, &qh)?;

        let pool = SlotPool::new(1, &shm)?;
        let layer_shell = LayerShell::bind(&globals, &qh)?;
        let monitors = vec![];

        Ok(Self {
            transition,
            compositor_state,
            registry_state: RegistryState::new(&globals),
            seat_state: SeatState::new(&globals, &qh),
            output_state: OutputState::new(&globals, &qh),
            event_queue: Some(event_queue),
            layer_shell,
            pool,
            shm,
            monitors
        })
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        let Some(mut evt_queue) = self.event_queue.take() else {
            return Ok(())
        };

        loop {
            evt_queue.blocking_dispatch(self)?;
        }
    }

    pub fn handle_new_output(
        &mut self,
        qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) -> anyhow::Result<()> {
        let output_info = self.output_state.info(&output).context("Failed to get output info")?;
        let monitor_name = output_info.name.context("Failed to get monitor_name")?;
        tracing::info!("Monitor Detected: {}", monitor_name);
        let (width, height) = output_info.logical_size.context("Failed to get monitor width and height")?;
        let surface = self.compositor_state.create_surface(qh);
        let layer = self.layer_shell.create_layer_surface(
            qh,
            surface,
            Layer::Background, 
            Some("background_layer"), 
            Some(&output)
        );
        layer.set_anchor(Anchor::BOTTOM);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_size(width as u32, height as u32);
        layer.commit();
        let monitor = Monitor { width, height, layer, configured: false };
        self.monitors.push(monitor);
        Ok(())
    }

    fn get_monitor(&mut self, surface: &WlSurface, configure: bool) -> Option<Monitor> {
        if configure {
            let monitor = self.monitors.iter_mut()
                .find(|m| m.layer.wl_surface() == surface)?;
            monitor.configured = configure;
            return Some(monitor.clone());
        }
        let monitor = self.monitors.iter()
            .find(|m| m.layer.wl_surface() == surface)?;
        Some(monitor.clone())
    }

    fn render(&mut self, qh: &QueueHandle<Self>, surface: &WlSurface, configure: bool) -> anyhow::Result<()> {
        // tracing::info!("RENDERING FRAME");
        let monitor = self.get_monitor(surface, configure).context("Monitor not found")?;
        if !monitor.configured {
            return Ok(())
        }

        if self.transition.is_finished() {
            monitor.layer.wl_surface().frame(qh, monitor.layer.wl_surface().clone());
            monitor.layer.commit();
            return Ok(())
        }

        let frame = self.transition.get_frame();

        let width = monitor.width;
        let height = monitor.height;
        let stride = width * 4;
        let (buffer, canvas) = self.pool.create_buffer(
            monitor.width,
            monitor.height,
            stride,
            wl_shm::Format::Argb8888
        )?;

        canvas.copy_from_slice(&frame);
        monitor.layer.wl_surface().damage_buffer(0, 0, width, height);
        monitor.layer.wl_surface().frame(qh, monitor.layer.wl_surface().clone());

        buffer.attach_to(monitor.layer.wl_surface())?;
        monitor.layer.commit();

        Ok(())
    }
}

impl CompositorHandler for WallpaperLayer {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
        // Not needed for this example.
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
        // Not needed for this example.
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        self.render(qh, surface, false).unwrap();
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
        // Not needed for this example.
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
        // Not needed for this example.
    }
}

impl OutputHandler for WallpaperLayer {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.handle_new_output(qh, output).unwrap();
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

impl LayerShellHandler for WallpaperLayer {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) { }

    fn configure(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        _configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        self.render(qh, layer.wl_surface(), true).unwrap();
    }
}

impl SeatHandler for WallpaperLayer {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _: Capability,
    ) {
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _: Capability,
    ) {
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for WallpaperLayer {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
    }

    fn press_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _event: KeyEvent,
    ) {
    }

    fn repeat_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: KeyEvent,
    ) {
        println!("Key repeat: {event:?}");
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        println!("Key release: {event:?}");
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _serial: u32,
        modifiers: Modifiers,
        _raw_modifiers: RawModifiers,
        _layout: u32,
    ) {
        println!("Update modifiers: {modifiers:?}");
    }
}

impl PointerHandler for WallpaperLayer {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        _events: &[PointerEvent],
    ) {
    }
}

impl ShmHandler for WallpaperLayer {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for WallpaperLayer {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}


delegate_compositor!(WallpaperLayer);
delegate_output!(WallpaperLayer);
delegate_seat!(WallpaperLayer);
delegate_keyboard!(WallpaperLayer);
delegate_pointer!(WallpaperLayer);
delegate_shm!(WallpaperLayer);

delegate_layer!(WallpaperLayer);

delegate_registry!(WallpaperLayer);


