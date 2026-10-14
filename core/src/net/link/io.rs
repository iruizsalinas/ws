//! Retain a shutdown handle while live, then release its descriptor promptly.
use crate::lock;
use std::net::{Shutdown, TcpStream};
use std::sync::Mutex;

pub(super) struct Io(Mutex<Option<TcpStream>>);

impl Io {
    pub fn new(tcp: TcpStream) -> Self {
        Self(Mutex::new(Some(tcp)))
    }

    pub fn shutdown(&self) {
        let tcp = lock(&self.0).take();
        if let Some(tcp) = tcp {
            let _ = tcp.shutdown(Shutdown::Both);
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/core/unit/io.rs"]
mod tests;
