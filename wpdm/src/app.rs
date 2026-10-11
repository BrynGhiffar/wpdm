use std::{path::Path, time::Instant};

use wpdm_common::{CliRequest, TransitionOrigin, TransitionType, WallpaperCmd, disk_state::DiskState};

use crate::{
    io::{
        event::{StartTransitionReq, TransitionParams, WpdmIoEvent, WpdmIoOutputEvent, WpdmIoRenderEvent, WpdmIoRenderRequest, WpdmIoRequest},
        wpdm_io::WpdmIo
    },
    transitions::manager::{Transition, TransitionManager},
    util::mmap_buffer
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

            // Validate the cache buffers before queueing the transition, so a
            // missing or wrongly-sized file cannot leave a stuck entry in
            // the manager (it would freeze the global min-frame for all
            // monitors).
            let expected_len = (oi.width * oi.height * 4) as usize;
            if !valid_cache(&from_path, expected_len) || !valid_cache(&to_path, expected_len) {
                continue;
            }

            let anim = TransitionParams::new(
                cmd.tran_type,
                cmd.tran_origin,
                cmd.angle,
                oi.width as u32,
                oi.height as u32
            );
            if !matches!(anim.tran_type, TransitionType::None) {
                self.trm.transitions.push(Transition {
                    frame: 0,
                    monitor: name.clone(),
                    params: anim
                });
            }

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

/// Check that the cached raw BGRA buffer for a wallpaper exists and has the
/// expected size for its monitor. Validation only: the mmap is dropped here,
/// the renderer re-maps the file when uploading.
fn valid_cache(path: &Path, expected_len: usize) -> bool {
    match mmap_buffer(path.to_path_buf()) {
        Ok(buf) if buf.len() == expected_len => true,
        Ok(buf) => {
            tracing::error!(
                "Failed to create transition, since buffer {path:?} has unexpected size: {}",
                buf.len()
            );
            false
        }
        Err(err) => {
            tracing::error!("Failed to mmap buffer {path:?}: {err}");
            false
        }
    }
}
