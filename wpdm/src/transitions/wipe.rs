use std::{f32::consts::PI, ops::Div};

use rayon::{iter::{IndexedParallelIterator,ParallelIterator}, slice::ParallelSliceMut};

use crate::util::{argb_buffer_size};

pub struct WipeTransition {
    n_frames: u32,
    width: u32,
    height: u32,
    angle: f32
}

pub struct WipeTransitionOpt {
    pub angle: f32,
    pub n_frames: u32
}

impl Default for WipeTransitionOpt {
    fn default() -> Self {
        Self { angle: 30.0, n_frames: 60 }
    }
}

impl WipeTransition {
    pub fn new_opt(width: u32, height: u32, opts: Option<WipeTransitionOpt>) -> Self {
        let opts = opts.unwrap_or_default();
        let angle: f32 = opts.angle.div(180.0) * PI;

        Self { width, height, angle, n_frames: opts.n_frames }
    }

    pub fn render(&self, frame: u32, from: &[u8], to: &[u8], result: &mut [u8]) -> bool {
        if frame > self.n_frames {
            return true;
        }
        assert_eq!(from.len(), argb_buffer_size(self.width, self.height) as usize);
        assert_eq!(to.len(), argb_buffer_size(self.width, self.height) as usize);
        assert_eq!(result.len(), argb_buffer_size(self.width, self.height) as usize);

        let usize_width = self.width as usize;
        let ca = self.angle.cos();
        let sa = self.angle.sin();
        let s = self.width as f32 * ca + self.height as f32 * sa;
        let n_frames = self.n_frames as f32;
        let frame = frame as f32;
        result
            .par_chunks_mut(4)
            .enumerate()
            .for_each(|(i, chunk)| {
                let x = (i % usize_width) as f32;
                let y = (i / usize_width) as f32;
                let d = (x * ca) + (y * sa);
                let pixel_p = d / s;
                let start = i * 4;
                let end = start + 4;
                if pixel_p <= frame.div(n_frames) {
                    chunk.copy_from_slice(&to[start..end]);
                } else {
                    chunk.copy_from_slice(&from[start..end]);
                }
            });
        false
    }
}

