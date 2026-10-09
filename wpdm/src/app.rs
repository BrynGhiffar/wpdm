use std::time::Instant;

use wayland_client::protocol::wl_shm;
use wpdm_common::{CliRequest, WallpaperCmd, disk_state::DiskState};

use crate::{io::{event::{StartTransitionReq, WpdmIoEvent, WpdmIoOutputEvent, WpdmIoRenderEvent, WpdmIoRenderRequest, WpdmIoRequest}, wpdm_io::WpdmIo}, transitions::{anim::TransitionAnim, manager::TransitionManager}};

const FRAME_INTERVAL: std::time::Duration = std::time::Duration::from_millis(1000 / 1001);

pub struct WpdmApp {
    io: WpdmIo,
    trm: TransitionManager,
    last_render: Instant,
}

impl WpdmApp {
    pub fn new() -> anyhow::Result<Self> {
        let io = WpdmIo::new()?;
        let trm = TransitionManager::new();

        Ok(WpdmApp { io, trm, last_render: Instant::now() })
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        loop {
            let events = self.io.poll()?;
            // tracing::info!("{:#?}", events);
            let reqs = events
                .into_iter()
                .flat_map(|evt| self.logic(evt))
                .collect::<Vec<_>>();
            for req in reqs {
                self.io.send(req)?;
            }
        }
    }

    pub fn logic(&mut self, event: WpdmIoEvent) -> Vec<WpdmIoRequest> {
        match event {
            WpdmIoEvent::Render(event) => self.render_logic(event),
            WpdmIoEvent::ConfigureOutput(event) => self.configure_logic(event),
            WpdmIoEvent::NewOutput(event) => self.new_output_logic(event),
            WpdmIoEvent::DestroyOutput(event) => self.destroy_output_logic(event),
            WpdmIoEvent::CliRequest(event) => self.cli_request_logic(event),
        }
    }

    pub fn render_logic(&mut self, event: WpdmIoRenderEvent) -> Vec<WpdmIoRequest> {
        let WpdmIoRenderEvent { mut slot, oi } = event;
        let (buffer, canvas) = match slot
            .create_buffer(
                oi.width,
                oi.height,
                oi.width * 4,
                wl_shm::Format::Argb8888
            ) {
                Ok(res) => res,
                Err(err) => {
                    tracing::error!("Failed to create buffer in rendering logic: {err}");
                    return vec![];
                }
            };
        if let Some(()) = self.trm.render_transition(&oi.name, canvas) {
            let has_transitions = self.trm.has_transitions();
            let elapsed = self.last_render.elapsed();
            if elapsed < FRAME_INTERVAL {
                std::thread::sleep(FRAME_INTERVAL - elapsed);
            }
            self.last_render = Instant::now();
            return vec![WpdmIoRequest::Render( WpdmIoRenderRequest {
                slot,
                buffer,
                oi,
                render_next: has_transitions
            })];
        }
        vec![]
    }

    pub fn configure_logic(&mut self, event: WpdmIoOutputEvent) -> Vec<WpdmIoRequest> {
        tracing::info!("Running configure logic");
        let WpdmIoOutputEvent { oi } = event;
        let Ok(path) = DiskState::get_curr_wp(&oi.name) else {
            return vec![];
        };

        let anim = TransitionAnim::none(
            oi.width as u32,
            oi.height as u32
        );

        vec![WpdmIoRequest::StartTransition(StartTransitionReq {
            oi,
            from: None,
            to: path,
            anim
        })]
    }

    pub fn new_output_logic(&mut self, _: WpdmIoOutputEvent) -> Vec<WpdmIoRequest> {
        vec![]
    }

    pub fn destroy_output_logic(&mut self, _: WpdmIoOutputEvent) -> Vec<WpdmIoRequest> {
        vec![]
    }

    pub fn cli_request_logic(&mut self, event: CliRequest) -> Vec<WpdmIoRequest> {
        match event {
            CliRequest::MonitorQuery => vec![WpdmIoRequest::CliMonitorQuery],
            CliRequest::WallpaperCmd(cmd) => self.wp_change_logic(cmd)
        }
    }

    pub fn wp_change_logic(&mut self, cmd: WallpaperCmd) -> Vec<WpdmIoRequest> {
        let mut reqs = vec![];
        for name in cmd.monitors {
            let from_path = DiskState::get_curr_wp(&name)
                .unwrap_or_else(|_| cmd.path.clone());

            let to_path = cmd.path.clone();
            if let Err(err) = DiskState::try_save_wp(&name, &cmd.path) {
                tracing::error!("Failed to save wallpaper: {err}");
            }

            let Some(oi) = self.io.get_oi_by_name(&name) else {
                continue;
            };

            let anim = match cmd.tran_type {
                wpdm_common::TransitionType::Wipe => TransitionAnim::wipe(
                    oi.width as u32,
                    oi.height as u32,
                    cmd.angle,
                    cmd.tran_origin
                ),
                wpdm_common::TransitionType::Circle => TransitionAnim::circle(
                    oi.width as u32,
                    oi.height as u32,
                    cmd.tran_origin
                ),
                wpdm_common::TransitionType::None => TransitionAnim::none(
                    oi.width as u32,
                    oi.height as u32
                ),
            };

            reqs.push(WpdmIoRequest::StartTransition(StartTransitionReq {
                oi,
                from: Some(from_path),
                to: to_path,
                anim
            }));
        }
        reqs
    }
}
