use std::sync::mpsc;
use std::thread::JoinHandle;

use anyhow::Context;
use wpdm_common::disk_state::DiskState;
use wpdm_common::{CliRequest, TransitionOrigin, TransitionType, WallpaperCmd, WpdmListener, WpdmMonitor};

use crate::cmd::RenderCmd;
use crate::{layer::SharedMonitorMeta};

pub struct WpdmServer {
    // Needs to know dimensions of the buffer to send
    listener: WpdmListener,
    producer: mpsc::SyncSender<RenderCmd>,
    monitor_meta: SharedMonitorMeta,
}

impl WpdmServer {
    pub fn new(
        producer: mpsc::SyncSender<RenderCmd>,
        monitor_meta: SharedMonitorMeta,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            listener: WpdmListener::new()?,
            producer,
            monitor_meta,
        })
    }

    pub fn wait_for_monitors(&self) {
        while self.monitor_meta.read().unwrap().is_empty() {
            std::thread::sleep(std::time::Duration::from_secs(1))
        }
    }

    pub fn handle_change_wallpaper(&mut self, cmd: WallpaperCmd) -> anyhow::Result<()> {
        // For each monitor:
        // 1. Get current wallpaper on each monitor
        // 2. Create a render command from the monitor's current wallpaper
        // 3. To the new wallpaper from the command
        for mon in cmd.monitors {
            let src_argb_buff_path = DiskState::get_curr_wp(&mon)
                .unwrap_or_else(|_| cmd.path.clone());

            let dest_argb_buff_path = cmd.path.clone();
            self.producer.send(RenderCmd {
                monitor: mon.to_string(),
                src_argb_buff_path,
                dest_argb_buff_path,
                tr_type: cmd.tran_type.clone(),
                tr_origin: cmd.tran_origin,
                angle: cmd.angle
            })
            .inspect_err(|e| tracing::error!("Failed sending buffer: {}", e))?;

            DiskState::try_save_wp(&mon, &cmd.path)?;
        }


        Ok(())
    }

    pub fn on_start(&mut self) -> anyhow::Result<()> {
        self.wait_for_monitors();
        let monitors: Vec<_> = { 
            let metas = self.monitor_meta.read().unwrap();
            metas.iter().map(|mm| mm.name.clone()).collect()
        };

        for mon in monitors.iter() {
            let path = DiskState::get_curr_wp(mon)?;
            self.producer.send(RenderCmd {
                monitor: mon.to_string(),
                src_argb_buff_path: path.clone(),
                dest_argb_buff_path: path.clone(),
                tr_origin: TransitionOrigin::Center,
                tr_type: TransitionType::None,
                angle: 0.0,
            })
            .inspect_err(|e| tracing::error!("Failed sending buffer: {}", e))?;
        }
        Ok(())
    }

    fn run_aux(mut self) {
        let _ = self.on_start().inspect_err(|e| tracing::error!("{}", e));

        loop {
            let Ok(message) = self.listener.poll()
                .inspect_err(|err| tracing::error!("Error when polling: {}", err)) else {
                continue;
            };

            match message {
                CliRequest::WallpaperCmd(set_wallpaper) => {
                    if let Err(err) = self.handle_change_wallpaper(set_wallpaper) {
                        tracing::error!("Error during change wallpaper: {}", err);
                    }
                },
                CliRequest::MonitorQuery => {
                    let monitor_metas = self.monitor_meta.read().unwrap();
                    let monitors = monitor_metas.iter()
                        .map(|mm| WpdmMonitor { name: mm.name.clone(), height: mm.height, width: mm.width })
                        .collect::<Vec<_>>();
                    let _ = self.listener.monitors(monitors)
                        .inspect_err(|err| tracing::error!("Failed to send monitors: {}", err));
                },
            };
        }
    }

    pub fn run(self) -> WpdmServerHandle {
        let handle = std::thread::spawn(move || self.run_aux());
        WpdmServerHandle(handle)
    }
}

pub struct WpdmServerHandle(JoinHandle<()>);

impl WpdmServerHandle {
    pub fn wait(self) -> anyhow::Result<()> {
        let Self(handle) = self;
        handle.join()
            .ok()
            .context("Issue in running joining WpdmServer thread")
    }
}
