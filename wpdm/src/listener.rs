use std::{
    collections::{BTreeMap},
    thread::JoinHandle,
};

use anyhow::Context;
use rtrb::PushError;
use wpdm_common::{WpdmListener, WpdmSetWallpaper};

use crate::{
    layer::{SharedMonitorMeta},
    wp_loader::WpLoader,
};

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

    pub fn handle_change_wallpaper(&mut self, sw: WpdmSetWallpaper) -> anyhow::Result<()> {
        // 1. Generate all frames for wallpaper change
        // 2. Fetch current wallpaper (need wallpaper image loader, since we don't store current
        //    wallpaper in memory)
        // 3. Generate frame transitions between set_wallpaper
        let mut hm = BTreeMap::<(i32, i32), Vec<&str>>::new();
        let monitors = self.monitor_meta.read().unwrap();
        tracing::info!("monitors: {}", monitors.len());
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
            tracing::info!("Buffer size: {}", buffer.len());
            let mut wp_buffer = WpBuffer {
                monitors: mons.into_iter().map(|v| v.to_string()).collect(),
                buffer,
                width,
                height,
            };

            while let Err(PushError::Full(wp_buff)) = self.producer.push(wp_buffer) {
                wp_buffer = wp_buff;
                tracing::error!("Frame buffer is full")
            }
        }

        Ok(())
    }

    pub fn on_start(&self) {
        // Need to set default wallpaper
    }

    pub fn run(mut self) -> anyhow::Result<WpdmServerHandle> {
        self.on_start();
        let handle = std::thread::spawn(move || {
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
