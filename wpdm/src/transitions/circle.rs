use rayon::{iter::{IndexedParallelIterator,ParallelIterator}, slice::ParallelSliceMut};

use crate::util::{argb_buffer_size};
use simsimd::SpatialSimilarity;

pub struct CircleTransition {
    n_frames: u32,
    width: u32,
    height: u32,
    origin_x: f32,
    origin_y: f32,
    max_radius: f32,
}

pub struct CircleTransitionOpt {
    pub origin_x: f32,
    pub origin_y: f32,
    pub n_frames: u32
}

impl Default for CircleTransitionOpt {
    fn default() -> Self {
        Self { origin_x: 0.0, origin_y: 0.0, n_frames: 60 }
    }
}

impl CircleTransition {
    pub fn new_opt(width: u32, height: u32, opts: Option<CircleTransitionOpt>) -> Self {
        let f32_width = width as f32;
        let f32_height = height as f32;
        let opts = opts.unwrap_or_default();
        let origin_x = opts.origin_x;
        let origin_y = opts.origin_y;

        let max_radius = [
            (0.0, 0.0),
            (0.0, f32_height),
            (f32_width, 0.0),
            (f32_width, f32_height)
        ]
        .into_iter()
        .flat_map(|(x, y)| f32::l2sq(&[x, y], &[origin_x, origin_y])).fold(f64::NEG_INFINITY, f64::max) as f32;

        CircleTransition { width, height, origin_x, origin_y, max_radius, n_frames: opts.n_frames }
    }

    pub fn render(&self, frame: u32, from: &[u8], to: &[u8], result: &mut [u8]) -> bool {
        if frame > self.n_frames {
            return true;
        }
        assert_eq!(from.len(), argb_buffer_size(self.width, self.height) as usize);
        assert_eq!(to.len(), argb_buffer_size(self.width, self.height) as usize);
        assert_eq!(result.len(), argb_buffer_size(self.width, self.height) as usize);

        let usize_width = self.width as usize;
        let f32_origin_x = self.origin_x;
        let f32_origin_y = self.origin_y;
        let radius = ((frame as f32) * self.max_radius) / (self.n_frames as f32);
        result
            .par_chunks_mut(4)
            .enumerate()
            .for_each(|(i, chunk)| {
                let x = (i % usize_width) as f32;
                let y = (i / usize_width) as f32;
                let crad = f32::l2sq(&[x, y], &[f32_origin_x, f32_origin_y]).unwrap_or(0.0) as f32;
                let use_new = radius.total_cmp(&crad).eq(&std::cmp::Ordering::Greater);
                let start = i * 4;
                let end = start + 4;
                if use_new {
                    chunk.copy_from_slice(&to[start..end]);
                } else {
                    chunk.copy_from_slice(&from[start..end]);
                }
            });
        false
    }
}

