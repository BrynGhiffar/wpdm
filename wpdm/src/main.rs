//! wpdm - A wallpaper daemon for wayland

mod layer;
mod listener;
mod loader;
mod transitions;
mod renderer;
mod util;
mod handler;

use std::sync::mpsc;

use crate::{layer::WallpaperLayer, listener::WpdmServer};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();

    let (prod, cons) = mpsc::sync_channel(1);

    let mut layer = WallpaperLayer::new(cons)?;
    let server = WpdmServer::new(prod, layer.get_monitor_meta())?;

    let handle = server.run();

    layer.run()?;
    handle.wait()?;
    Ok(())
}
