use std::collections::HashMap;
use clap::Parser;
use wpdm_common::WpdmMonitor;

use crate::cache::CachedImg;
mod cache;

#[derive(Parser)]
struct Args {
    #[arg(short, long)]
    image_path: String,
}

fn group_by_size(monitors: Vec<WpdmMonitor>) -> impl Iterator<Item = (i32, i32, Vec<String>)> {
    let sizes = monitors.into_iter()
        .fold(HashMap::<(i32, i32), Vec<String>>::new(), |mut init, nxt| {
        if let Some(monitors) = init.get_mut(&(nxt.width, nxt.height)) {
            monitors.push(nxt.name);
        } else {
            init.insert((nxt.width, nxt.height), vec![nxt.name]);
        }
        init
    });
    sizes.into_iter().map(|((width, height), monitors) | (width, height, monitors))
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();
    let args = Args::parse();
    let mut client = wpdm_common::WpdmClient::new()?;

    let mut cached_img = CachedImg::new(&args.image_path)?;

    // Get all active monitors
    let monitors = client.get_monitors()?;

    // Group monitors by size
    for (width, height, monitors) in group_by_size(monitors) {
        let img_path = cached_img.get_image(width, height)?;

        // Set monitors according to their sizes
        client.set_wallpaper(img_path, monitors)?;
    }

    Ok(())
}

