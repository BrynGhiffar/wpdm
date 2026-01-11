use std::io::Write;
use std::{collections::BTreeMap, fs::OpenOptions, io::Read, path::PathBuf, thread::JoinHandle};

use anyhow::Context;
use rtrb::PushError;
use wpdm_common::{WpdmListener, WpdmSetWallpaper};

use crate::{layer::SharedMonitorMeta, wp_loader::WpLoader};

pub struct WpBuffer {
    // Buffer will be in argb form
    pub monitors: Vec<String>,
    pub buffer: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

// A wallpaper is a single frame.
// A wallpaper transition is a function T that takes two wallpapers W and a time t, where t => 0..1 and returns a
// new frame or wallpaper, since w = f, T(w1, w2, t) = w_t
//

pub struct WpdmServer {
    // Needs to know dimensions of the buffer to send
    listener: WpdmListener,
    producer: rtrb::Producer<WpBuffer>,
    monitor_meta: SharedMonitorMeta,
}

impl WpdmServer {
    pub fn new(
        port: Option<u16>,
        producer: rtrb::Producer<WpBuffer>,
        monitor_meta: SharedMonitorMeta,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            listener: WpdmListener::new(port)?,
            producer,
            monitor_meta,
        })
    }

    pub fn config_path() -> Option<PathBuf> {
        Some(
            std::env::home_dir()?
                .join(".local")
                .join("state")
                .join("wpdm")
                .join("config.conf"),
        )
    }

    pub fn config_dir() -> Option<PathBuf> {
        Self::config_path()?.parent().map(|p| p.to_path_buf())
    }

    pub fn wait_for_monitors(&self) {
        while self.monitor_meta.read().unwrap().is_empty() {
            std::thread::sleep(std::time::Duration::from_millis(20))
        }
    }

    pub fn handle_change_wallpaper(&mut self, sw: WpdmSetWallpaper) -> anyhow::Result<()> {
        // 1. Generate all frames for wallpaper change
        // 2. Fetch current wallpaper (need wallpaper image loader, since we don't store current
        //    wallpaper in memory)
        // 3. Generate frame transitions between set_wallpaper
        let mut hm = BTreeMap::<(i32, i32), Vec<&str>>::new();
        let monitors = self.monitor_meta.read().unwrap();
        for mon in monitors.iter() {
            if let Some(ent) = hm.get_mut(&(mon.width, mon.height)) {
                ent.push(&mon.name);
            } else {
                hm.insert((mon.width, mon.height), vec![&mon.name]);
            }
        }

        for ((width, height), mons) in hm.into_iter() {
            let image = WpLoader::config(&sw.path, width as u32, height as u32).load()?;
            let buffer = image.buffer;
            let width = image.width;
            let height = image.height;
            let mut wp_buffer = WpBuffer {
                monitors: mons.into_iter().map(|v| v.to_string()).collect(),
                buffer,
                width,
                height,
            };

            while let Err(PushError::Full(wp_buff)) = self.producer.push(wp_buffer) {
                wp_buffer = wp_buff;
            }
        }

        std::fs::create_dir_all(Self::config_dir().context("Failed to get config dir")?)?;
        let mut save = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(Self::config_path().context("Failed to open config file")?)?;

        writeln!(save, "{}", &sw.path)?;

        Ok(())
    }

    pub fn on_start(&mut self) -> anyhow::Result<()> {
        let path = Self::config_path().context("Failed to get config path")?;
        // std::fs::create_dir_all(&path).ok()?;
        let mut data_file = OpenOptions::new().read(true).open(path)?;
        let mut path = String::new();
        let _ = data_file.read_to_string(&mut path)?;
        let path = path.trim().to_string();
        self.wait_for_monitors();
        tracing::info!("Finished waiting for monitors, {:?}", &path);
        self.handle_change_wallpaper(WpdmSetWallpaper { path })?;
        Ok(())
    }

    pub fn run(mut self) -> anyhow::Result<WpdmServerHandle> {
        let handle = std::thread::spawn(move || {
            let _ = self.on_start().inspect_err(|e| tracing::error!("{}", e));

            loop {
                let Some(message) = self.listener.poll() else {
                    continue;
                };

                match message {
                    wpdm_common::WpdmMessage::SetWallpaper(set_wallpaper) => {
                        if let Err(err) = self.handle_change_wallpaper(set_wallpaper) {
                            tracing::error!("Error during change wallpaper: {}", err);
                        }
                    }
                };
            }
        });
        Ok(WpdmServerHandle(handle))
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
