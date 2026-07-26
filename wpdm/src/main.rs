//! wpdm - A wallpaper daemon for wayland

mod layer;
mod loader;
mod transitions;
mod util;
mod handler;
mod cmd;
mod io;
mod app;

use crate::app::WpdmApp;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();
    let mut app = WpdmApp::new()?;
    app.run()?;
    Ok(())
}
