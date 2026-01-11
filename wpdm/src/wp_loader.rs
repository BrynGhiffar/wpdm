use gcd::Gcd;
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

    fn center(&self, mut img: image::DynamicImage) -> image::DynamicImage {
        let gcd = self.mon_width.gcd(self.mon_height);
        let mon_ar_width = self.mon_width / gcd;
        let mon_ar_height = self.mon_height / gcd;

        let gcd = img.width().gcd(img.height());
        let img_ar_width = img.width() / gcd;
        let img_ar_height = img.height() / gcd;

        let is_wide = img_ar_width * mon_ar_height >= mon_ar_width * img_ar_height;

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
            let height = (img.width() * mon_ar_height) / mon_ar_width;

            let x = 0;
            let y = img.height() / 2 - height / 2;
            img = img.crop(x, y, width, height);
        }

        if !dim_equals {
            img = img.resize(
                self.mon_width,
                self.mon_height,
                image::imageops::FilterType::Gaussian,
            );
        }

        img
    }

    pub fn load(self) -> anyhow::Result<WpBuffer> {
        let img = ImageReader::open(self.path.as_ref())?
            .with_guessed_format()?
            .decode()
            .inspect_err(|err| tracing::error!("Error when loading image: {}", err))?;
        // let img = image::open(self.path.as_ref()).inspect_err(|err| tracing::error!("Error when loading image: {}", err))?;
        let img = self.center(img);
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
