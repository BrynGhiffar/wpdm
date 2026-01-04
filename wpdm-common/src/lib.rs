use std::{io::{Read, Write}, net::{TcpListener, TcpStream}};


#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct WpdmSetWallpaper {
    path: String
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub enum WpdmMessage {
    SetWallpaper(WpdmSetWallpaper)
}

const SUCCESS: [u8; 4] = [1; 4];
const DEFAULT_PORT: u16 = 64647;

impl WpdmMessage {
    pub fn set_wallpaper(path: String) -> Self {
        Self::SetWallpaper(WpdmSetWallpaper { path })
    }
}

pub struct WpdmClient {
    stream: TcpStream
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
        let message = WpdmMessage::set_wallpaper(path);
        postcard::to_io(&message, &mut self.stream)
            .inspect_err(|err| tracing::error!("Failed to send set wallpaper: {}", err))?;
        self.stream.shutdown(std::net::Shutdown::Write)?;
        let mut buffer = [0; 4];
        let amount = self.stream.read(&mut buffer)?;
        Ok((buffer == SUCCESS) && (amount == buffer.len()))
    }
}

pub struct WpdmListener {
    listener: TcpListener
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
        tracing::info!("Polling for connections");
        let (mut stream, incoming_addr)  = self.listener.accept().ok()?;

        tracing::info!("Connection created from: {}", incoming_addr);
        let mut bytes = Vec::new();

        let amt = stream.read_to_end(&mut bytes).ok()?;

        tracing::info!("amount: {:?}", amt);

        let result = postcard::from_bytes(&bytes[..amt])
            .inspect_err(|err| tracing::error!("Error when deserializing: {}", err)).ok();

        tracing::info!("Result is: {:?}", result);
        let _ = stream.write(&[1; 4]).ok()?;
        result
    }
}
