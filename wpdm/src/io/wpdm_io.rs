use std::os::fd::{AsFd, AsRawFd};

use mio::{Events, Interest, Poll, Token, unix::SourceFd};
use wayland_client::{backend::WaylandError, EventQueue};
use wpdm_common::{CliRequest, CliResponse, serde_udp::SerdeUdp};

use crate::io::{event::{WpdmIoEvent, WpdmIoRequest, WpdmOutputInfo}, layer_io::WpdmLayerIO};


pub struct WpdmIo {
    layer_io: WpdmLayerIO,
    layer_evq: EventQueue<WpdmLayerIO>,
    poll: Poll,
    events: Events,
    listener: SerdeUdp<CliResponse, CliRequest>,
}

const LAYER_TOKEN: Token = Token(0);
const UDP_TOKEN: Token = Token(1);

impl WpdmIo {
    pub fn new() -> anyhow::Result<Self> {
        let (mut layer_io, mut layer_evq) = WpdmLayerIO::new()?;
        let poll = Poll::new()?;
        let events = Events::with_capacity(32);
        let wl_fd = layer_io.conn.as_fd();
        let wl_fd = wl_fd.as_raw_fd();
        let listener = SerdeUdp::server()?;
        let udp_fd = listener.as_fd();
        let udp_fd = udp_fd.as_raw_fd();
        layer_evq.roundtrip(&mut layer_io)?;

        poll.registry()
            .register(&mut SourceFd(&wl_fd), LAYER_TOKEN, Interest::READABLE)?;

        poll.registry()
            .register(&mut SourceFd(&udp_fd), UDP_TOKEN, Interest::READABLE)?;

        Ok(Self { layer_io, layer_evq, poll, events, listener })
    }

    pub fn pop_layer_io_evt(&mut self, res: &mut Vec<WpdmIoEvent>) {
        while let Some(evt) = self.layer_io.io_queue.pop_front() {
            res.push(evt);
        }
    }

    pub fn poll(&mut self) -> anyhow::Result<Vec<WpdmIoEvent>> {
        let mut res = vec![];

        self.layer_evq.flush()?;
        self.layer_evq.dispatch_pending(&mut self.layer_io)?;

        if let Some(guard) = self.layer_evq.prepare_read() {
            tracing::info!("Polling...");
            loop {
                match self.poll.poll(&mut self.events, None) {
                    Ok(()) => break,
                    Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {
                        tracing::info!("System call interrupted by signal (EINTR). Retrying...");
                        continue;
                    },
                    Err(e) => return Err(e.into())
                }
            }
            tracing::info!("Received some events...");

            if self.events.iter().any(|e| e.token() == LAYER_TOKEN) {
                match guard.read() {
                    Ok(_) => {
                        self.layer_evq.dispatch_pending(&mut self.layer_io)?;
                    }
                    Err(WaylandError::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        tracing::info!("Spurious wakeup on wayland socket");
                    }
                    Err(e) => return Err(e.into()),
                }
            }

            if self.events.iter().any(|e| e.token() == UDP_TOKEN) {
                let req = self.listener.recv()?;
                res.push(WpdmIoEvent::CliRequest(req));
            }
        }
        self.pop_layer_io_evt(&mut res);
        Ok(res)
    }

    pub fn send(&mut self, req: WpdmIoRequest) -> anyhow::Result<()> {
        match req {
            WpdmIoRequest::StartTransition(req) => self.layer_io.start_transition(req)?,
            WpdmIoRequest::InitRender(oi) => self.layer_io.init_render(oi)?,
            WpdmIoRequest::Render(req) =>  self.layer_io.render(req)?,
            WpdmIoRequest::CliMonitorQuery => self.listener.send(
                CliResponse::Monitors(self.layer_io.monitor_query())
            )?
            
        }
        Ok(())
    }

    pub fn get_oi_by_name(&self, name: &str) -> Option<WpdmOutputInfo> {
        self.layer_io.get_output_by_name(name).map(|out| out.to_oi())
    }
}
