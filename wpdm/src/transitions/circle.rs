use rayon::iter::{IndexedParallelIterator, IntoParallelRefIterator, IntoParallelRefMutIterator, ParallelIterator};

use crate::transitions::blit::blit;
use crate::util::argb_buffer_size;

#[derive(Debug, Clone)]
pub struct CircleTransition {
    n_frames: u32,
    width: u32,
    height: u32,
    max_radius: f32,
    dist_sq: Vec<f32>,
}

pub struct CircleTransitionOpt {
    pub origin_x: f32,
    pub origin_y: f32,
    pub n_frames: u32,
}

impl Default for CircleTransitionOpt {
    fn default() -> Self {
        Self {
            origin_x: 0.0,
            origin_y: 0.0,
            n_frames: 60,
        }
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
            (0.0f32, 0.0f32),
            (0.0, f32_height),
            (f32_width, 0.0),
            (f32_width, f32_height),
        ]
        .iter()
        .map(|(x, y)| {
            let dx = x - origin_x;
            let dy = y - origin_y;
            (dx * dx + dy * dy).sqrt()
        })
        .fold(0.0f32, f32::max);

        let pixels = (width * height) as usize;
        let mut dist_sq = Vec::with_capacity(pixels);
        for y in 0..height {
            for x in 0..width {
                let dx = x as f32 - origin_x;
                let dy = y as f32 - origin_y;
                dist_sq.push(dx * dx + dy * dy);
            }
        }

        CircleTransition {
            width,
            height,
            max_radius,
            n_frames: opts.n_frames,
            dist_sq,
        }
    }

    pub fn render(&self, frame: u32, from: &[u8], to: &[u8], result: &mut [u8]) -> bool {
        assert_eq!(from.len(), argb_buffer_size(self.width, self.height) as usize);
        assert_eq!(to.len(), argb_buffer_size(self.width, self.height) as usize);
        assert_eq!(result.len(), argb_buffer_size(self.width, self.height) as usize);

        let radius = ((frame as f32) * self.max_radius) / (self.n_frames as f32);
        let radius_sq = radius * radius;

        let pixels = (self.width * self.height) as usize;
        let mut mask = vec![0u8; pixels];
        mask.par_iter_mut()
            .zip(self.dist_sq.par_iter())
            .for_each(|(m, d)| *m = if *d <= radius_sq { 1 } else { 0 });

        blit(from, to, &mask, result);
        frame >= self.n_frames
    }
}
