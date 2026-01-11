use std::{
    net::{TcpListener, TcpStream},
    path::Path,
};

use anyhow::Context;

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct WpdmSetWallpaper {
    pub path: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub enum WpdmMessage {
    SetWallpaper(WpdmSetWallpaper),
}

const DEFAULT_PORT: u16 = 64647;

impl WpdmMessage {
    pub fn set_wallpaper(path: String) -> Self {
        Self::SetWallpaper(WpdmSetWallpaper { path })
    }
}

pub struct WpdmClient {
    stream: TcpStream,
}

impl WpdmClient {
    pub fn new(port: Option<u16>) -> anyhow::Result<Self> {
        let port = port.unwrap_or(DEFAULT_PORT);
        let host = "127.0.0.1";
        let addr = format!("{}:{}", host, port);
        tracing::info!("Connecting to {}", &addr);
        let stream = TcpStream::connect(addr)
            .inspect_err(|err| tracing::error!("Failed to connect: {}", err))?;
        Ok(Self { stream })
    }

    pub fn set_wallpaper(&mut self, path: String) -> anyhow::Result<bool> {
        let path = Path::new(&path).to_path_buf().canonicalize()?;
        let path = path
            .to_str()
            .context("Failed to convert canonicalized path to string")?
            .to_string();
        let message = WpdmMessage::set_wallpaper(path);
        postcard::to_io(&message, &mut self.stream)
            .inspect_err(|err| tracing::error!("Failed to send set wallpaper: {}", err))?;
        self.stream.shutdown(std::net::Shutdown::Both)?;
        Ok(true)
    }
}

pub struct WpdmListener {
    listener: TcpListener,
}

impl WpdmListener {
    pub fn new(port: Option<u16>) -> anyhow::Result<Self> {
        let port = port.unwrap_or(DEFAULT_PORT);
        let host = "0.0.0.0";
        let addr = format!("{}:{}", host, port);
        let listener = TcpListener::bind(&addr)?;
        tracing::info!("Listening on: {}", addr);
        // listener.set_nonblocking(true)?;
        Ok(Self { listener })
    }

    pub fn poll(&self) -> Option<WpdmMessage> {
        tracing::info!("Waiting for connection...");
        let (mut stream, incoming_addr) = self.listener.accept().ok()?;

        tracing::info!("Received connection from from: {}", incoming_addr);
        let mut bytes = [0;1024];

        let (res, _) = postcard::from_io::<WpdmMessage, _>((&mut stream, &mut bytes)).ok()?;

        Some(res)
    }
}
