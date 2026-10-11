use std::{fs::OpenOptions, path::PathBuf};

use memmap2::Mmap;

pub fn mmap_buffer(path: PathBuf) -> anyhow::Result<memmap2::Mmap> {
    let file = OpenOptions::new()
        .read(true)
        .open(path)?;

    let mmap = unsafe { Mmap::map(&file)? };

    Ok(mmap)
}
