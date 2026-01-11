//! wpdm - A wallpaper daemon for wayland

mod layer;
mod listener;
mod loader;

use std::sync::mpsc;

use crate::{layer::WallpaperLayer, listener::WpdmServer};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();

    // let (prod, cons) = rtrb::RingBuffer::new(1);
    let (prod, cons) = mpsc::sync_channel(1);

    let mut layer = WallpaperLayer::new(cons)?;
    let server = WpdmServer::new(None, prod, layer.get_monitor_meta())?;

    let handle = server.run()?;
    layer.run()?;
    handle.wait()?;
    Ok(())
}
