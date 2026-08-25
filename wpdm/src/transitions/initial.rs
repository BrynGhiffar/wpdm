use rayon::{iter::{IndexedParallelIterator,ParallelIterator}, slice::ParallelSliceMut};

use crate::util::{argb_buffer_size};

#[derive(Debug, Clone)]
pub struct NoTransition {
    width: u32,
    height: u32,
}

impl NoTransition {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub fn render(&self, frame: u32, from: &[u8], to: &[u8], result: &mut [u8]) -> bool {
        assert_eq!(from.len(), argb_buffer_size(self.width, self.height) as usize);
        assert_eq!(to.len(), argb_buffer_size(self.width, self.height) as usize);
        assert_eq!(result.len(), argb_buffer_size(self.width, self.height) as usize);

        result
            .par_chunks_mut(4)
            .enumerate()
            .for_each(|(i, chunk)| {
                let start = i * 4;
                let end = start + 4;
                chunk.copy_from_slice(&to[start..end]);
            });
        frame == 0
    }
}

