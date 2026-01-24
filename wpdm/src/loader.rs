use image::ImageReader;
use rayon::{iter::ParallelIterator, slice::ParallelSliceMut};
use std::path::Path;

use crate::listener::WpBuffer;

pub struct WpLoader<P: AsRef<Path>> {
    path: P,
    mon_width: u32,
    mon_height: u32,
}

impl<P: AsRef<Path>> WpLoader<P> {
    pub fn config(path: P, mon_width: u32, mon_height: u32) -> Self {
        WpLoader {
            path,
            mon_width,
            mon_height,
        }
    }

    pub fn load(self) -> anyhow::Result<WpBuffer> {
        let img = ImageReader::open(self.path.as_ref())?
            .with_guessed_format()?
            .decode()
            .inspect_err(|err| tracing::error!("Error when loading image: {}", err))?;
        let mut buffer = img.to_rgba8().to_vec();

        buffer.par_chunks_exact_mut(4).for_each(|buff| {
            let r = buff[0];
            let g = buff[1];
            let b = buff[2];
            let a = buff[3];
            buff.copy_from_slice(&[b, g, r, a]);
        });

        Ok(WpBuffer {
            monitors: vec![],
            buffer,
            width: self.mon_width,
            height: self.mon_height,
        })
    }
}
