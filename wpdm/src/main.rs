//! wpdm - A wallpaper daemon for wayland

mod image_transition;
mod layer;
mod listener;
mod wallpaper;

use crate::{layer::WallpaperLayer, listener::WpdmServer};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();

    let (prod, cons) = rtrb::RingBuffer::new(1);

    let mut layer = WallpaperLayer::new(&args)?;
    let server = WpdmServer::new(None, prod)?;

    let handle = server.run()?;
    layer.run()?;
    handle.wait()?;
    Ok(())
}
