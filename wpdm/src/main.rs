//! wpdm - A wallpaper daemon for wayland

mod image_transition;
mod layer;
mod listener;
mod wallpaper;

use crate::{layer::WallpaperLayer, listener::WpdmServer};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();
    tracing::info!("Starting WPDM");
    let mut args = std::env::args();
    args.next();
    let args = args.collect::<Vec<_>>();

    let mut layer = WallpaperLayer::new(&args)?;
    let server = WpdmServer::new(None)?;

    let handle = server.run()?;
    layer.run()?;
    handle.wait()?;
    Ok(())
}
