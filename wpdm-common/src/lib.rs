pub mod serde_udp;
pub mod config;
use std::net::TcpStream;

use anyhow::anyhow;

use crate::serde_udp::SerdeUdp;

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct WpdmSetWallpaper {
    pub path: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct WpdmMonitor {
    pub name: String,
    pub height: i32,
    pub width: i32
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct WpdmMonitors {
    pub monitors: Vec<WpdmMonitor>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub enum WpdmMessage {
    SetWallpaper(WpdmSetWallpaper),
    QueryMonitor,
    Monitors(WpdmMonitors)
}

pub trait WpdmStream {
    fn send_wpdm(&mut self, message: &WpdmMessage) -> anyhow::Result<()>;
    fn recv_wpdm(&mut self) -> anyhow::Result<WpdmMessage>;
}

impl WpdmStream for mio::net::TcpStream {
    fn send_wpdm(&mut self, message: &WpdmMessage) -> anyhow::Result<()> {
        let _ = postcard::to_io(message, self)?;
        Ok(())
    }

    fn recv_wpdm(&mut self) -> anyhow::Result<WpdmMessage> {
        const MAX_SIZE: usize = 1000 * 1024;
        let mut bytes = [0; MAX_SIZE];
        let (res, _) = postcard::from_io::<WpdmMessage, _>((self, &mut bytes))?;
        Ok(res)
    }
}

impl WpdmStream for TcpStream {
    fn send_wpdm(&mut self, message: &WpdmMessage) -> anyhow::Result<()> {
        let _ = postcard::to_io(message, self)?;
        Ok(())
    }

    fn recv_wpdm(&mut self) -> anyhow::Result<WpdmMessage> {
        const MAX_SIZE: usize = 1000 * 1024;
        let mut bytes = [0; MAX_SIZE];
        let (res, _) = postcard::from_io::<WpdmMessage, _>((self, &mut bytes))?;
        Ok(res)
    }
}

impl WpdmMessage {
    pub fn set_wallpaper(path: String) -> Self {
        Self::SetWallpaper(WpdmSetWallpaper { path })
    }
}

pub struct WpdmClient {
    stream: SerdeUdp<WpdmMessage>,
}

// Problems:
// 1. We want to process image on the client.
// 2. How do we tell the client about the monitor size?
// 3. Need bidirectional communication. (Server also needs to be able to send message to client)
// 4. Means server has to manage connection to clients?
//      No, server can just use broadcast, will assume that there is only a single client.

impl WpdmClient {
    pub fn new() -> anyhow::Result<Self> {
        let stream = SerdeUdp::client()?;
        Ok(Self { stream })
    }

    pub fn set_wallpaper(&mut self, path: String) -> anyhow::Result<()> {
        let message = WpdmMessage::set_wallpaper(path);

        self.stream.send(message)
            .inspect_err(|err| tracing::error!("Failed to send set wallpaper: {}", err))?;

        Ok(())
    }

    pub fn get_monitors(&mut self) -> anyhow::Result<Vec<WpdmMonitor>> {
        self.stream.send(WpdmMessage::QueryMonitor)
            .inspect_err(|err| tracing::error!("Failed to send set wallpaper: {}", err))?;

        let message = self.stream.recv()?;

        let WpdmMessage::Monitors(WpdmMonitors { monitors }) = message else {
            return Err(anyhow!("Server didn't return correct response"));
        };

        Ok(monitors)
    }
}

pub struct WpdmListener {
    listener: SerdeUdp<WpdmMessage>,
}

impl WpdmListener {
    pub fn new() -> anyhow::Result<Self> {
        let listener = SerdeUdp::server()?;
        Ok(Self { listener })
    }

    pub fn monitors(&mut self, monitors: Vec<WpdmMonitor>) -> anyhow::Result<()> {
        let message = WpdmMessage::Monitors(WpdmMonitors { monitors });
        self.listener.send(message)?;
        Ok(())
    }

    pub fn poll(&mut self) -> anyhow::Result<WpdmMessage> {
        Ok(self.listener.recv()?)
    }
}
