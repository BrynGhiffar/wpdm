use std::io::Write;
use std::sync::mpsc;
use std::{collections::BTreeMap, fs::OpenOptions, io::Read, path::PathBuf, thread::JoinHandle};

use anyhow::Context;
use rayon::iter::{IndexedParallelIterator, ParallelIterator};
use rayon::slice::{ParallelSlice, ParallelSliceMut};
use wpdm_common::{WpdmListener, WpdmSetWallpaper};

use crate::{layer::SharedMonitorMeta, loader::WpLoader};

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
//
//
struct GrowCircleTransition {
    f: u32,
    curr: Option<WpBuffer>,
    next: Option<WpBuffer>,
}

impl Iterator for GrowCircleTransition {
    type Item = WpBuffer;
    fn next(&mut self) -> Option<Self::Item> {
        let curr = self.curr.as_ref()?;
        let next = self.next.as_ref()?;
        // assume 60 fps
        // 4 second animation
        // need to complete animation in 240 frames
        if self.f == 240 {
            return self.next.take()
        }
        let f = self.f as i32;
        let width = curr.width as i32;
        let height = curr.height as i32;
        let center_x = (curr.width / 2) as i32;
        let center_y = (curr.height / 2) as i32;

        let dist = |(x1, y1), (x2, y2)| {
            let x = x1 - x2;
            let y = y1 - y2;
            let res: i32 = x * x + y * y;
            res.isqrt()
        };

        let max_diam = [(0, 0), (0, height), (width, 0), (width, height)].into_iter().map(|(x, y)| dist((center_x, center_y), (x, y))).max().unwrap();
        let diam = (f * max_diam) / 240;

        let mut output_buffer = vec![0; (width * height * 4) as usize];
        output_buffer.par_chunks_mut(4)
            .zip(curr.buffer.par_chunks(4))
            .zip(next.buffer.par_chunks(4))
            .enumerate()
            .for_each(|(i, ((out, curr), nxt))| {
                let x = (i as i32) % width;
                let y = (i as i32) / width;
                let d = dist((center_x, center_y), (x, y));

                if d <= diam {
                    out.copy_from_slice(nxt);
                } else {
                    out.copy_from_slice(curr);
                }
            });

        let res = WpBuffer { monitors: vec![], buffer: output_buffer, width: curr.width, height: curr.height };

        self.f += 1;
        Some(res)
    }
}

pub struct WpdmServer {
    // Needs to know dimensions of the buffer to send
    listener: WpdmListener,
    producer: mpsc::SyncSender<WpBuffer>,
    monitor_meta: SharedMonitorMeta,
}

impl WpdmServer {
    pub fn new(
        port: Option<u16>,
        producer: mpsc::SyncSender<WpBuffer>,
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
            let mut image = WpLoader::config(&sw.path, width as u32, height as u32).load()?;
            // Need to generate transition frames here, an animation/transition is just an iterator
            // of WpBuffer's
            if let Ok(curr_path) = self.get_curr_wp_path() {
                let curr = WpLoader::config(&curr_path, width as u32, height as u32).load()?;
                let transition = GrowCircleTransition { f: 0, curr: Some(curr), next: Some(image) };

                for mut wp_buffer in transition {
                    wp_buffer.monitors = mons.iter().map(|v| v.to_string()).collect();
                    let _ = self.producer.send(wp_buffer)
                        .inspect_err(|e| tracing::error!("Failed sending buffer: {}", e));
                }
                
                continue
            }
            image.monitors = mons.into_iter().map(|v| v.to_string()).collect();
            let _ = self.producer.send(image)
                .inspect_err(|e| tracing::error!("Failed sending buffer: {}", e));
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

    pub fn get_curr_wp_path(&self) -> anyhow::Result<String> {
        let path = Self::config_path().context("Failed to get config path")?;
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
