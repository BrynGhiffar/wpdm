use std::{fs::{File, OpenOptions}, io::Read, path::PathBuf};

use memmap2::Mmap;

pub fn load_argb_buffer(path: PathBuf) -> anyhow::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut contents = Vec::new();
    let _ = file.read_to_end(&mut contents)?;
    Ok(contents)
}

pub fn mmap_buffer(path: PathBuf) -> anyhow::Result<memmap2::Mmap> {
    let file = OpenOptions::new()
        .read(true)
        .open(path)?;

    let mmap = unsafe { Mmap::map(&file)? };

    Ok(mmap)
}
