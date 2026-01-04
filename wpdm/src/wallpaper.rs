use std::path::Path;

use rayon::iter::{IndexedParallelIterator, IntoParallelRefIterator, IntoParallelRefMutIterator, ParallelIterator};

use crate::image_transition::get_velocity;

struct FadeInAnimation {
    frame_count: u8,
    velocity: Vec<i8>,
    is_finished: bool,
}

impl FadeInAnimation {
    fn create(curr_wallpaper: &[u8], nxt_wallpaper: &[u8]) -> Self {
        let velocity = get_velocity(curr_wallpaper, nxt_wallpaper);

        Self {
            is_finished: false,
            velocity,
            frame_count: 0
        }
    }

    pub fn frame(&mut self, curr_wp: &[u8], nxt_wp: &[u8], frame: &mut [u8], frame_count: u8) {
        curr_wp.par_iter()
            .zip(nxt_wp)
            .zip(self.velocity.par_iter())
            .zip(frame.par_iter_mut())
            .for_each(|(((a, b), v), f)| {
                let a = *a as i16;
                let b = *b as i16;
                let v = *v as i16;
                let frame = frame_count as i16;
                let c = a + v * frame;

                let lower = std::cmp::min(a, b);
                let higher = std::cmp::max(a, b);

                *f = std::cmp::max(lower, std::cmp::min(c, higher)) as u8;
            });
    }
}

pub fn load_current_wallpaper(width: u32, height: u32) -> Vec<u8> {
    let counts = width * height * 4;
    vec![0; counts as usize]
}

pub fn load_wallpaper<P: AsRef<Path>>(path: P, width: u32, height: u32) -> Vec<u8> {
    let counts = width * height * 4;
    vec![0; counts as usize]
}

pub struct Wallpaper {
    curr_wallpaper: Option<Vec<u8>>,
    next_wallpaper: Option<Vec<u8>>,
    animation: Option<FadeInAnimation>,
}

impl Wallpaper {
    pub fn new() -> Self {
        Self { 
            curr_wallpaper: None,
            next_wallpaper: None, 
            animation: None
        }
    }

    pub fn handle_set_wallpaper(&mut self) {
        // 1. Create a new image transition
    }

    pub fn frame(&mut self, _frame: &mut [u8]) -> bool {
        false
        // 1. Check message buffer
        // 2. Create a new image transition if there is a new message
        //  3. If there is a new set image, create a new image transition to replace previous
        //  4. Return frame from image transition and return true
        // 2. If there is no new message, return the existing buffer
        //  3. If there is an existing transition, return frame from the existing transition and
        //     return true
        //  4. If there is no existing transition, just return false

    }
}
