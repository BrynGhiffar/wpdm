use std::fs::OpenOptions;
use std::{collections::HashMap, path::Path};
use anyhow::Context;
use clap::Parser;
use fast_image_resize::{images::Image, ResizeOptions, Resizer};
use fast_image_resize::IntoImageView;
use image::codecs::png::PngEncoder;
use image::{ImageEncoder, ImageReader};
use gcd::Gcd;
use sha2::{Digest, Sha256};
use wpdm_common::config;
use std::fmt::Write;

#[derive(Parser)]
struct Args {
    #[arg(short, long)]
    image_path: String,
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

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();
    let args = Args::parse();
    let mut client = wpdm_common::WpdmClient::new()?;

    let path = Path::new(&args.image_path).to_path_buf().canonicalize()?;

    let img = ImageReader::open(path)?.decode()?;
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

    for ((width, height), _) in sizes {
        let mut dst_image = Image::new(width as u32, height as u32, img.pixel_type().context("Image does not have pixel type")?);
        let mut resizer = Resizer::new();
        let (left, top, rwidth, rheight) = get_crop_params(width as u32, height as u32, img.width(), img.height());
        resizer.resize(&img, &mut dst_image, &ResizeOptions::new().crop(left as f64, top as f64, rwidth as f64, rheight as f64))?;

        let digest = Sha256::digest(dst_image.buffer());

        let mut filename = String::new();

        write!(&mut filename, "{:x}", digest)?;
        let _ = filename.split_off(10);
        filename.push_str(".png");
        let path = config::config_dir().context("Cannot get config dir")?.join(&filename);

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)?;

        let path = path.to_path_buf().canonicalize()?;

        PngEncoder::new(&mut file)
            .write_image(dst_image.buffer(), width as u32, height as u32, img.color().into())?;

        let str_path = path.to_str().context("Cannot convert path to string")?.to_string();
        println!("Writing image to {}", &str_path);
        client.set_wallpaper(str_path)?;
    }


    Ok(())
}
