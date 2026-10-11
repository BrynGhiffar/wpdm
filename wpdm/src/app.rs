use std::time::Instant;

use wpdm_common::{CliRequest, TransitionOrigin, TransitionType, WallpaperCmd, disk_state::DiskState};

use crate::{
    io::{
        event::{StartTransitionReq, TransitionParams, WpdmIoEvent, WpdmIoOutputEvent, WpdmIoRenderEvent, WpdmIoRenderRequest, WpdmIoRequest},
        wpdm_io::WpdmIo
    },
    transitions::manager::{Transition, TransitionManager}
};

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
        let WpdmIoRenderEvent { oi } = event;
        let Some((params, progress)) = self.trm.advance(&oi.name) else {
            return vec![];
        };

        let has_transitions = self.trm.has_transitions();
        let elapsed = self.last_render.elapsed();
        if elapsed < FRAME_INTERVAL {
            std::thread::sleep(FRAME_INTERVAL - elapsed);
        }
        self.last_render = Instant::now();

        vec![WpdmIoRequest::Render(WpdmIoRenderRequest {
            oi,
            params,
            progress,
            render_next: has_transitions
        })]
    }

    pub fn configure_logic(&mut self, event: WpdmIoOutputEvent) -> Vec<WpdmIoRequest> {
        tracing::info!("Running configure logic");
        let WpdmIoOutputEvent { oi } = event;
        let Ok(path) = DiskState::get_curr_wp(&oi.name) else {
            return vec![];
        };

        let anim = TransitionParams::new(
            TransitionType::None,
            TransitionOrigin::Center,
            0.0,
            oi.width as u32,
            oi.height as u32
        );

        vec![WpdmIoRequest::StartTransition(StartTransitionReq {
            oi,
            from: None,
            to: path,
            anim,
            sim: false
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
            // Read the current wallpaper before overwriting the saved state:
            // it becomes the transition source (`from`), unless it is the
            // same image, in which case a hard cut is equivalent.
            let old_path = DiskState::get_curr_wp(&name).ok();
            let new_path = cmd.path.clone();
            if let Err(err) = DiskState::try_save_wp(&name, &cmd.path) {
                tracing::error!("Failed to save wallpaper: {err}");
            }

            let Some(oi) = self.io.get_oi_by_name(&name) else {
                continue;
            };

            let anim = TransitionParams::new(
                cmd.tran_type,
                cmd.tran_origin,
                cmd.angle,
                oi.width as u32,
                oi.height as u32
            );
            // Animated transitions are useless in sim mode (the renderer
            // declines them), so only queue manager entries for the static
            // path.
            if !cmd.sim && !matches!(anim.tran_type, TransitionType::None) {
                self.trm.transitions.push(Transition {
                    frame: 0,
                    monitor: name.clone(),
                    params: anim
                });
            }

            reqs.push(WpdmIoRequest::StartTransition(StartTransitionReq {
                oi,
                from: old_path.filter(|old| old != &new_path),
                to: new_path,
                anim,
                sim: cmd.sim
            }));
        }
        reqs
    }
}
