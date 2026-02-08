pub mod serde_udp;
pub mod config;
pub mod disk_state;

use std::path::PathBuf;

use crate::serde_udp::SerdeUdp;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy)]
pub enum TransitionType {
    Circle,
    Wipe,
    None
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy)]
pub enum TransitionOrigin {
    Center,
    Left,
    Right,
    Random,
    Coord(u64, u64),
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct WallpaperCmd {
    pub path: PathBuf,
    pub monitors: Vec<String>,
    pub tran_type: TransitionType,
    pub tran_origin: TransitionOrigin,
    pub angle: f32,
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
pub enum CliRequest {
    WallpaperCmd(WallpaperCmd),
    MonitorQuery,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub enum CliResponse {
    Monitors(WpdmMonitors)
}

pub struct WpdmClient {
    stream: SerdeUdp<CliRequest, CliResponse>,
}

impl WpdmClient {
    pub fn new() -> anyhow::Result<Self> {
        let stream = SerdeUdp::client()?;
        Ok(Self { stream })
    }

    pub fn send(&mut self, req: CliRequest) -> anyhow::Result<()> {
        self.stream.send(req)?;
        Ok(())
    }

    pub fn get_monitors(&mut self) -> anyhow::Result<Vec<WpdmMonitor>> {
        let CliResponse::Monitors(mon) = self.stream.send_recv(CliRequest::MonitorQuery)?;
        Ok(mon.monitors)
    }
}

pub struct WpdmListener {
    listener: SerdeUdp<CliResponse, CliRequest>,
}

impl WpdmListener {
    pub fn new() -> anyhow::Result<Self> {
        let listener = SerdeUdp::server()?;
        Ok(Self { listener })
    }

    pub fn monitors(&mut self, monitors: Vec<WpdmMonitor>) -> anyhow::Result<()> {
        let message = CliResponse::Monitors(WpdmMonitors { monitors });
        self.listener.send(message)?;
        Ok(())
    }

    pub fn poll(&mut self) -> anyhow::Result<CliRequest> {
        Ok(self.listener.recv()?)
    }
}
