use std::path::Path;

use anyhow::Context;
use image::DynamicImage;

/// wgpu's default `max_texture_dimension_2d`.
const MAX_TEXTURE_DIM: u32 = 8192;
/// Transient texture budget; larger decodes are pre-scaled to fit. One 4K
/// BGRA frame is ~32 MB — beyond that the GPU composite downsizes anyway,
/// and the budget bounds the staging allocation `write_texture` makes
/// (the allocator retains it after the upload).
const MAX_TEXTURE_BYTES: u64 = 32 * 1024 * 1024;

/// Decode a source image into BGRA bytes at native resolution (no resize:
/// the GPU scales and crops at draw time), capped to the texture budget.
pub fn decode_bgra(path: &Path) -> anyhow::Result<(Vec<u8>, u32, u32)> {
    let img = image::ImageReader::open(path)?
        .with_guessed_format()?
        .decode()
        .with_context(|| format!("failed to decode {path:?}"))?;
    let img = cap(img);
    let (width, height) = (img.width(), img.height());
    Ok((rgba_to_bgra(img), width, height))
}

/// Pre-scale a decoded image into the texture budget via the rare path:
/// only ran for images exceeding the max texture dimension or size.
fn cap(img: DynamicImage) -> DynamicImage {
    let (width, height) = (img.width(), img.height());
    let bytes = width as u64 * height as u64 * 4;
    if width <= MAX_TEXTURE_DIM && height <= MAX_TEXTURE_DIM && bytes <= MAX_TEXTURE_BYTES {
        return img;
    }
    let dim_scale = (MAX_TEXTURE_DIM as f32) / (width.max(height) as f32);
    let byte_scale = ((MAX_TEXTURE_BYTES as f32) / (bytes as f32)).sqrt();
    let scale = dim_scale.min(byte_scale);
    img.resize_exact(
        (width as f32 * scale) as u32,
        (height as f32 * scale) as u32,
        image::imageops::FilterType::Triangle,
    )
}

/// The image crate has no BGRA DynamicImage variant, so swap the R and B
/// channels; matching the -cli's conversion byte for byte.
fn rgba_to_bgra(img: DynamicImage) -> Vec<u8> {
    let mut buf = img.into_rgba8().into_vec();
    for px in buf.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    buf
}
