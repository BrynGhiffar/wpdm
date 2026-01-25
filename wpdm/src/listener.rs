use std::path::Path;
use std::sync::mpsc;
use std::{fs::OpenOptions, io::Read, thread::JoinHandle};

use anyhow::Context;
use wpdm_common::config::save_wp_path;
use wpdm_common::{config, WpdmListener, WpdmMonitor, WpdmSetWallpaper};

use crate::layer::RenderCommand;
use crate::{layer::SharedMonitorMeta};

pub struct WpdmServer {
    // Needs to know dimensions of the buffer to send
    listener: WpdmListener,
    producer: mpsc::SyncSender<RenderCommand>,
    monitor_meta: SharedMonitorMeta,
}

impl WpdmServer {
    pub fn new(
        producer: mpsc::SyncSender<RenderCommand>,
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

    pub fn handle_change_wallpaper(&mut self, sw: WpdmSetWallpaper) -> anyhow::Result<()> {
        // 1. Generate all frames for wallpaper change
        // 2. Fetch current wallpaper (need wallpaper image loader, since we don't store current
        //    wallpaper in memory)
        // 3. Generate frame transitions between set_wallpaper
        let src_argb_buff_path = Path::new(&self.get_curr_wp_path()?).to_owned();
        let dest_argb_buff_path = Path::new(&sw.path).to_owned();
        let monitors = sw.monitors;

        self.producer.send(RenderCommand::Transition {
            monitors,
            src_argb_buff_path,
            dest_argb_buff_path
        })
        .inspect_err(|e| tracing::error!("Failed sending buffer: {}", e))?;

        // TODO: Save path needs to run on wpdm-cli
        save_wp_path(&sw.path)?;

        Ok(())
    }

    pub fn get_curr_wp_path(&self) -> anyhow::Result<String> {
        let path = config::config_path().context("Failed to get config path")?;
        let mut data_file = OpenOptions::new().read(true).open(path)?;
        let mut path = String::new();
        let _ = data_file.read_to_string(&mut path)?;
        let path = path.trim().to_string();
        Ok(path)
    }

    pub fn on_start(&mut self) -> anyhow::Result<()> {
        let path = self.get_curr_wp_path()?;
        self.wait_for_monitors();
        tracing::info!("Finished waiting for monitors, {:?}", &path);
        let monitors = { 
            let metas = self.monitor_meta.read().unwrap();
            metas.iter().map(|mm| mm.name.clone()).collect()
        };
        self.handle_change_wallpaper(WpdmSetWallpaper { path, monitors })?;
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
                wpdm_common::WpdmMessage::SetWallpaper(set_wallpaper) => {
                    if let Err(err) = self.handle_change_wallpaper(set_wallpaper) {
                        tracing::error!("Error during change wallpaper: {}", err);
                    }
                },
                wpdm_common::WpdmMessage::QueryMonitor => {
                    let monitor_metas = self.monitor_meta.read().unwrap();
                    let monitors = monitor_metas.iter()
                        .map(|mm| WpdmMonitor { name: mm.name.clone(), height: mm.height, width: mm.width })
                        .collect::<Vec<_>>();
                    let _ = self.listener.monitors(monitors)
                        .inspect_err(|err| tracing::error!("Failed to send monitors: {}", err));
                },

                // Client side messages
                wpdm_common::WpdmMessage::Monitors(_) => { }
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
        self.0
            .join()
            .ok()
            .context("Issue in running joining WpdmServer thread")
    }
}
