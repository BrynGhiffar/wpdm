use std::thread::JoinHandle;

use anyhow::Context;
use wpdm_common::{WpdmListener, WpdmSetWallpaper};

pub struct WpdmServer {
    listener: WpdmListener
}

impl WpdmServer {
    pub fn new(port: Option<u16>) -> anyhow::Result<Self> {
        Ok(Self { listener: WpdmListener::new(port)? })
    }

    pub fn handle_change_wallpaper(&self, _set_wallpaper: WpdmSetWallpaper) {
        // 1. Change Wallpaper
    }


    pub fn run(self) -> anyhow::Result<WpdmServerHandle> {
        let handle = std::thread::spawn(move || {
            loop {
                let Some(message) = self.listener.poll() else {
                    continue;
                };

                match message {
                    wpdm_common::WpdmMessage::SetWallpaper(set_wallpaper) => 
                        self.handle_change_wallpaper(set_wallpaper),
                }

            }
        });
        Ok(WpdmServerHandle(handle))
    }
}

pub struct WpdmServerHandle(JoinHandle<()>);

impl WpdmServerHandle {

    pub fn wait(self) -> anyhow::Result<()> {
        self.0.join().ok().context("Issue in running joining WpdmServer thread")
    }
}
