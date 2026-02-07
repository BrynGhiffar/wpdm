use std::path::{Path, PathBuf};
use std::fmt::Write;

use anyhow::Context;
use image::{DynamicImage, ImageReader};
use sha2::{Digest, Sha256};
use wpdm_common::config;

use crate::{build_bgra_buffer, cache_exists};


pub struct CachedImg {
    path: PathBuf,
    loaded_image: Option<DynamicImage>
}

fn get_cache_name(path: &str, width: i32, height: i32) -> anyhow::Result<String> {
    let digest = Sha256::digest(path);
    let mut digest_str = String::new();
    write!(&mut digest_str, "{}x{}_{:x}", width, height, digest)?;
    let _ = digest_str.split_off(20);
    digest_str.push_str(".bgra");
    Ok(digest_str)
}

impl CachedImg {
    fn new<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref().canonicalize()?;
        Ok(Self { path, loaded_image: None })
    }

    fn get_image(&mut self, width: i32, height: i32) -> anyhow::Result<String>{
        let full_path = self.path.to_str().context("Failed to get string")?;
        let cache_name = get_cache_name(full_path, width, height)?;
        let cache_path = config::config_dir().context("Failed to get config dir")?.join(&cache_name);
        let cache_exists = cache_exists(&cache_name);
        if !cache_exists && let Some(imgg) = self.loaded_image.as_ref() {
            build_bgra_buffer(imgg, width as u32, height as u32, &cache_path)?;
        } else if !cache_exists {
            let imgg = ImageReader::open(&self.path)?.with_guessed_format()?.decode()?;
            build_bgra_buffer(&imgg, width as u32, height as u32, &cache_path)?;
            self.loaded_image.replace(imgg);
        }

        let str_path = cache_path.to_str().context("Cannot convert path to string")?.to_string();
        Ok(str_path)
    }
}
