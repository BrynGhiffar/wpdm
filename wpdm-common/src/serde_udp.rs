use std::{marker::PhantomData, net::UdpSocket, os::fd::AsFd};

use serde::{de::DeserializeOwned, Serialize};


const SERVER_ADDR: &str = "127.0.0.1:50100";
const CLIENT_ADDR: &str = "127.0.0.1:50101";

pub struct SerdeUdp<Req, Res, const B: usize = 1024> {
    socket: UdpSocket,
    req_marker: PhantomData<Req>,
    res_marker: PhantomData<Res>,
    buffer: [u8; B]
}

impl<Req, Res, const B: usize>  AsFd for SerdeUdp<Req, Res, B> {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.socket.as_fd()
    }
}


#[derive(thiserror::Error, Debug)]
pub enum SerdeUdpErr {

    #[error(transparent)]
    PostcardErr(#[from] postcard::Error),

    #[error(transparent)]
    IoErr(#[from] std::io::Error),
}

impl<Req, Res, const B: usize> SerdeUdp<Req, Res, B> where Req: Serialize + DeserializeOwned, Res: Serialize + DeserializeOwned {
    pub fn server() -> std::io::Result<Self> {
        Ok(Self {
            socket: UdpSocket::bind(SERVER_ADDR)?,
            req_marker: PhantomData,
            res_marker: PhantomData,
            buffer: [0; B]
        })
    }

    pub fn client() -> std::io::Result<Self> {
        Ok(Self {
            socket: UdpSocket::bind(CLIENT_ADDR)?,
            req_marker: PhantomData,
            res_marker: PhantomData,
            buffer: [0; B]
        })
    }

    pub fn find_peers(&self) -> Vec<String> {
        let Ok(local_sock) = self.socket.local_addr() else {
            return vec![];
        };

        let local_sock = local_sock.to_string();

        vec![SERVER_ADDR, CLIENT_ADDR]
            .into_iter()
            .filter(|ss| **ss != local_sock)
            .map(|s| s.to_string())
            .collect()
    }

    pub fn send(&mut self, data: Req) -> Result<(), SerdeUdpErr> {
        let peers = self.find_peers();
        let buff = postcard::to_slice::<Req>(&data, &mut self.buffer)?;
        for peer in peers {
            let _ = self.socket.send_to(buff, &peer)?;
        }
        Ok(())
    }

    pub fn recv(&mut self) -> Result<Res, SerdeUdpErr> {
        let (size, _) = self.socket.recv_from(&mut self.buffer)?;
        let out = postcard::from_bytes::<Res>(&self.buffer[..size])?;
        Ok(out)
    }

    pub fn send_recv(&mut self, data: Req) -> Result<Res, SerdeUdpErr> {
        self.send(data)?;
        self.recv()
    }
}
