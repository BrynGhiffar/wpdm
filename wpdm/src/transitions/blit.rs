use rayon::{iter::{IndexedParallelIterator, ParallelIterator}, slice::ParallelSliceMut};

pub fn blit(from: &[u8], to: &[u8], mask: &[u8], result: &mut [u8]) {
    result
        .par_chunks_mut(4)
        .enumerate()
        .for_each(|(i, chunk)| {
            let start = i * 4;
            if mask[i] != 0 {
                chunk.copy_from_slice(&to[start..start + 4]);
            } else {
                chunk.copy_from_slice(&from[start..start + 4]);
            }
        });
}
