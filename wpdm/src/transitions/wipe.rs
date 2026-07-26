use std::f32::consts::PI;

use rayon::iter::{IndexedParallelIterator, IntoParallelRefIterator, IntoParallelRefMutIterator, ParallelIterator};

use crate::transitions::blit::blit;
use crate::util::argb_buffer_size;

pub struct WipeTransition {
    n_frames: u32,
    width: u32,
    height: u32,
    projected: Vec<f32>,
}

pub struct WipeTransitionOpt {
    pub angle: f32,
    pub n_frames: u32,
}

impl Default for WipeTransitionOpt {
    fn default() -> Self {
        Self {
            angle: 30.0,
            n_frames: 60,
        }
    }
}

impl WipeTransition {
    pub fn new_opt(width: u32, height: u32, opts: Option<WipeTransitionOpt>) -> Self {
        let opts = opts.unwrap_or_default();
        let angle_rad = opts.angle * PI / 180.0;

        let ca = angle_rad.cos();
        let sa = angle_rad.sin();
        let max_projected = width as f32 * ca + height as f32 * sa;

        let pixels = (width * height) as usize;
        let mut projected = Vec::with_capacity(pixels);
        for y in 0..height {
            for x in 0..width {
                let d = x as f32 * ca + y as f32 * sa;
                projected.push(d / max_projected);
            }
        }

        Self {
            width,
            height,
            n_frames: opts.n_frames,
            projected,
        }
    }

    pub fn render(&self, frame: u32, from: &[u8], to: &[u8], result: &mut [u8]) -> bool {
        assert_eq!(from.len(), argb_buffer_size(self.width, self.height) as usize);
        assert_eq!(to.len(), argb_buffer_size(self.width, self.height) as usize);
        assert_eq!(result.len(), argb_buffer_size(self.width, self.height) as usize);

        let threshold = (frame as f32) / (self.n_frames as f32);

        let pixels = (self.width * self.height) as usize;
        let mut mask = vec![0u8; pixels];
        mask.par_iter_mut()
            .zip(self.projected.par_iter())
            .for_each(|(m, p)| *m = if *p <= threshold { 1 } else { 0 });

        blit(from, to, &mask, result);
        frame >= self.n_frames
    }
}
