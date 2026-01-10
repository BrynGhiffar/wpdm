use std::path::PathBuf;
use std::{fs::OpenOptions, io::Read, path::Path, thread::JoinHandle};

use std::fmt::Write;

use anyhow::Context;
use lz4::{Decoder, EncoderBuilder};
use rayon::iter::ParallelIterator;
use rayon::slice::ParallelSliceMut;
use sha2::{Digest, Sha256};
use wpdm_common::{WpdmListener, WpdmSetWallpaper};

use gcd::Gcd;

pub struct WpBuffer {
    // Buffer will be in argb form
    buffer: Vec<u8>,
    width: u32,
    height: u32,
}

// A wallpaper is a single frame.
// A wallpaper transition is a function T that takes two wallpapers W and a time t, where t => 0..1 and returns a
// new frame or wallpaper, since w = f, T(w1, w2, t) = w_t
//

pub struct WpLoader<P: AsRef<Path>> {
    path: P,
    mon_width: u32,
    mon_height: u32,
    cache_loc: PathBuf
}

impl<P: AsRef<Path>> WpLoader<P> {
    fn config(path: P, mon_width: u32, mon_height: u32) -> Self {
        let cache_loc = Path::new("~/.wpdm_cache").to_path_buf();
        WpLoader { path, mon_width, mon_height, cache_loc } 
    }

    fn center(&self, mut img: image::DynamicImage) -> image::DynamicImage {
        let gcd = self.mon_width.gcd(self.mon_height);
        let mon_ar_width = self.mon_width / gcd;
        let mon_ar_height = self.mon_height / gcd;

        let gcd = img.width().gcd(img.height());
        let img_ar_width = img.width() / gcd;
        let img_ar_height = img.height() / gcd;

        let is_wide = img.width() >= img.height();

        let dim_equals = (self.mon_height == img.height()) && (self.mon_width == img.width());

        let ar_equals = (mon_ar_width == img_ar_width) && (mon_ar_height == img_ar_height);

        if is_wide && !ar_equals {
            let width = (img.height() * mon_ar_width) / mon_ar_height;
            let height = img.height();

            let x = img.width() / 2 - width / 2;
            let y = 0;
            img = img.crop(x, y, width, height);
        } else if !ar_equals {
            let width = img.width();
            let height = img.height();

            let x = 0;
            let y = img.height() / 2 - self.mon_height / 2;
            img = img.crop(x, y, width, height);
        }

        if !dim_equals {
            img = img.resize(
                self.mon_width, 
                self.mon_height, 
                image::imageops::FilterType::Gaussian
            );
        }

        img
    }

    fn load_cached(&self, hkey: &str) -> anyhow::Result<WpBuffer> {
        let cache_file = self.cache_loc.join(hkey);
        let input_file = OpenOptions::new()
            .read(true)
            .open(&cache_file)?;

        let mut decoder = Decoder::new(&input_file)?;
        let buffer_len = (self.mon_width * self.mon_height) as usize;
        let mut buffer = vec![0; buffer_len];
        std::io::copy(&mut decoder, &mut buffer)?;
        Ok(WpBuffer { buffer, width: self.mon_width, height: self.mon_height })
    }

    fn get_key(&self) -> Option<String> {
        let mut file = OpenOptions::new()
            .read(true)
            .open(self.path.as_ref())
            .ok()?;

        // Number of bytes to read determine cache key
        const BUFF_LEN: usize = 10_000;
        let mut buffer = [0; BUFF_LEN];
        let read_amt = file.read(&mut buffer).ok()?;
        // Dimensions matter
        let width = self.mon_width.to_le_bytes();
        let height = self.mon_height.to_le_bytes();

        let width_start = BUFF_LEN - width.len();
        let height_start = width_start - height.len();
        buffer[width_start..BUFF_LEN].copy_from_slice(&width);
        buffer[height_start..width_start].copy_from_slice(&height);

        let digest = Sha256::digest(&buffer[..read_amt]);
        let mut digest_str = String::new();
        write!(&mut digest_str, "{:x}", digest).ok()?;
        Some(digest_str)
    }

    fn save(&self, hkey: &str, mut input: &[u8]) -> anyhow::Result<()> {
        let cache_file = self.cache_loc.join(hkey);
        let output_file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(&cache_file)?;

        let mut encoder = EncoderBuilder::new()
            .level(4)
            .build(output_file)?;

        std::io::copy(&mut input, &mut encoder)?;
        Ok(())
    }

    fn load(self) -> anyhow::Result<WpBuffer> {
        let hkey = self.get_key().context("Failed to get hkey")?;
        if let Ok(buff) = self.load_cached(&hkey) {
            return Ok(buff)
        }
        let img = image::open(self.path.as_ref())?;
        let img = self.center(img);
        let mut buffer = img.to_rgba8().to_vec();

        buffer.par_chunks_exact_mut(4).for_each(|buff| {
            let r = buff[0];
            let g = buff[1];
            let b = buff[2];
            let a = buff[3];
            buff.copy_from_slice(&[b, g, r, a]);
        });

        self.save(&hkey, &buffer)?;

        Ok(WpBuffer { buffer, width: self.mon_width, height: self.mon_height })
    }
}

pub trait Transition: Send + Sync {
    // If returns none, then transition is finished and can be discarded
    fn frame(&mut self) -> Option<WpBuffer>;
}

pub struct WpdmServer {
    // Needs to know dimensions of the buffer to send
    listener: WpdmListener,
    producer: rtrb::Producer<WpBuffer>
}

impl WpdmServer {
    pub fn new(port: Option<u16>, producer: rtrb::Producer<WpBuffer>) -> anyhow::Result<Self> {
        Ok(Self { listener: WpdmListener::new(port)?, producer })
    }

    pub fn handle_change_wallpaper(&mut self, _set_wallpaper: WpdmSetWallpaper) {
        // 1. Generate all frames for wallpaper change
        // 2. Fetch current wallpaper (need wallpaper image loader, since we don't store current
        //    wallpaper in memory)
        // 3. Generate frame transitions between set_wallpaper
        let buffer = vec![];
        let width = 0;
        let height = 0;
        self.producer.push(WpBuffer { buffer, width, height });
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
                    wpdm_common::WpdmMessage::SetWallpaper(set_wallpaper) => 
                        self.handle_change_wallpaper(set_wallpaper),
                }

            }
        });
        Ok(WpdmServerHandle(handle))
    }
}

pub struct WpdmServerHandle(JoinHandle<()>);

impl WpdmServerHandle {

    pub fn wait(self) -> anyhow::Result<()> {
        self.0.join().ok().context("Issue in running joining WpdmServer thread")
    }
}
