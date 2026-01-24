use std::{marker::PhantomData, net::UdpSocket};

use serde::{de::DeserializeOwned, Serialize};


const SERVER_ADDR: &str = "127.0.0.1:50100";
const CLIENT_ADDR: &str = "127.0.0.1:50101";

pub struct SerdeUdp<T, const B: usize = 1024> {
    socket: UdpSocket,
    marker: PhantomData<T>,
    buffer: [u8; B]
}

#[derive(thiserror::Error, Debug)]
pub enum SerdeUdpErr {

    #[error(transparent)]
    PostcardErr(#[from] postcard::Error),

    #[error(transparent)]
    IoErr(#[from] std::io::Error),
}

impl<T, const B: usize> SerdeUdp<T, B> where T: Serialize + DeserializeOwned {
    pub fn server() -> std::io::Result<Self> {
        Ok(Self {
            socket: UdpSocket::bind(SERVER_ADDR)?,
            marker: PhantomData,
            buffer: [0; B]
        })
    }

    pub fn client() -> std::io::Result<Self> {
        Ok(Self {
            socket: UdpSocket::bind(CLIENT_ADDR)?,
            marker: PhantomData,
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

    pub fn send(&mut self, data: T) -> Result<(), SerdeUdpErr> {
        let peers = self.find_peers();
        let buff = postcard::to_slice::<T>(&data, &mut self.buffer)?;
        for peer in peers {
            let _ = self.socket.send_to(buff, &peer)?;
        }
        Ok(())
    }

    pub fn recv(&mut self) -> Result<T, SerdeUdpErr> {
        let (size, _) = self.socket.recv_from(&mut self.buffer)?;
        let out = postcard::from_bytes::<T>(&self.buffer[..size])?;
        Ok(out)
    }
}
