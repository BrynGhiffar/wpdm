use std::fs::OpenOptions;
use std::io::{BufWriter, Cursor, Write};
use std::{collections::HashMap, path::Path};
use anyhow::Context;
use clap::Parser;
use fast_image_resize::{images::Image, ResizeOptions, Resizer};
use fast_image_resize::{IntoImageView};
use image::codecs::png::PngEncoder;
use image::{DynamicImage, ImageEncoder, ImageReader};
use gcd::Gcd;
use rayon::iter::ParallelIterator;
use rayon::slice::ParallelSliceMut;
use sha2::{Digest, Sha256};
use wpdm_common::config;
use std::fmt::Write as FmtWrite;

#[derive(Parser)]
struct Args {
    #[arg(short, long)]
    image_path: String,
}

fn get_cache_name(path: &str, width: i32, height: i32) -> anyhow::Result<String> {
    let digest = Sha256::digest(path);
    let mut digest_str = String::new();
    write!(&mut digest_str, "{}x{}_{:x}", width, height, digest)?;
    let _ = digest_str.split_off(20);
    digest_str.push_str(".bgra");
    Ok(digest_str)
}

fn cache_exists(cache_name: &str) -> bool {
    let Some(dir) = config::config_dir() else {
        return false;
    };
    std::fs::exists(dir.join(cache_name)).unwrap_or(false)
}


fn build_bgra_buffer(img: &DynamicImage, width: u32, height: u32, cache_path: &Path) -> anyhow::Result<()> {
    let mut dst_image = Image::new(width, height, img.pixel_type().context("Image does not have pixel type")?);
    let mut resizer = Resizer::new();
    let (left, top, rwidth, rheight) = get_crop_params(width, height, img.width(), img.height());

    resizer.resize(img, &mut dst_image, &ResizeOptions::new().crop(left as f64, top as f64, rwidth as f64, rheight as f64))?;


    let mut result_buf = BufWriter::new(Vec::new());
    PngEncoder::new(&mut result_buf)
        .write_image(
            dst_image.buffer(),
            width,
            height,
            img.color().into(),
        )
        .unwrap();
    let png_image = result_buf.into_inner()?;
    let png_image = Cursor::new(png_image);
    let image = image::ImageReader::new(png_image);
    let dimage = image.with_guessed_format()?.decode()?;
    let mut image_vec = dimage.into_rgba8().into_vec();
    image_vec
        .par_chunks_exact_mut(4)
        .for_each(|buff| {
            let r = buff[0];
            let g = buff[1];
            let b = buff[2];
            let a = buff[3];
            buff.copy_from_slice(&[b, g, r, a]);
        });

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(cache_path)?;


    let _ = file.write(&image_vec)?;

    Ok(())
}



fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();
    let args = Args::parse();
    let mut client = wpdm_common::WpdmClient::new()?;
    let image_path = Path::new(&args.image_path).canonicalize()?;
    let image_path_str = image_path.to_str().context("Failed to get string")?;

    let monitors = client.get_monitors()?;
    let sizes = monitors.into_iter()
        .fold(HashMap::<(i32, i32), Vec<String>>::new(), |mut init, nxt| {
        if let Some(monitors) = init.get_mut(&(nxt.width, nxt.height)) {
            monitors.push(nxt.name);
        } else {
            init.insert((nxt.width, nxt.height), vec![nxt.name]);
        }
        init
    });

    let mut img = None;
    for ((width, height), monitors) in sizes {
        let cache_name = get_cache_name(image_path_str, width, height)?;
        let cache_path = config::config_dir().context("Cannot get config dir")?.join(&cache_name);
        let cache_exists = cache_exists(&cache_name);

        if !cache_exists && let Some(imgg) = img.as_ref() {
            build_bgra_buffer(imgg, width as u32, height as u32, &cache_path)?;
        } else if !cache_exists {
            let imgg = ImageReader::open(&image_path)?.with_guessed_format()?.decode()?;
            build_bgra_buffer(&imgg, width as u32, height as u32, &cache_path)?;
            img = Some(imgg);
        }

        let path = cache_path.to_path_buf().canonicalize()?;

        let str_path = path.to_str().context("Cannot convert path to string")?.to_string();
        client.set_wallpaper(str_path, monitors)?;
    }

    Ok(())
}

fn get_crop_params(mon_width: u32, mon_height: u32, img_width: u32, img_height: u32) -> (u32, u32, u32, u32) {
        let gcd = mon_width.gcd(mon_height);
        let mon_ar_width = mon_width / gcd;
        let mon_ar_height = mon_height / gcd;

        let gcd = img_width.gcd(img_height);
        let img_ar_width = img_width / gcd;
        let img_ar_height = img_height / gcd;

        let is_wide = img_ar_width * mon_ar_height >= mon_ar_width * img_ar_height;

        let ar_equals = (mon_ar_width == img_ar_width) && (mon_ar_height == img_ar_height);

        if is_wide && !ar_equals {
            let width = (img_height * mon_ar_width) / mon_ar_height;
            let height = img_height;

            let x = img_width / 2 - width / 2;
            let y = 0;
            return (x, y, width, height);
        } else if !ar_equals {
            let width = img_width;
            let height = (img_width * mon_ar_height) / mon_ar_width;

            let x = 0;
            let y = img_height / 2 - height / 2;
            return (x, y, width, height)
        }

        (0, 0, img_width, img_height)
}

