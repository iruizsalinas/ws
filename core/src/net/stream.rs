//! HTTP upgrade I/O, bounded by the caller's absolute handshake deadline.
use crate::proto::http;
use std::io::{self, ErrorKind};
use std::net::TcpStream;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub(crate) trait Io: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}
pub(crate) type Transport = Box<dyn Io>;

pub(crate) struct Stream {
    pub tcp: TcpStream,
    pub io: Transport,
    pub activity: Arc<super::activity::Activity>,
    pub flush: Arc<tokio::sync::Notify>,
    /// TCP benefits from larger writes; TLS retains 16 KiB flush batches.
    pub chunk: usize,
}

pub(crate) enum HeadError {
    Io(io::Error),
    TooLarge,
}

impl Stream {
    pub async fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        self.io.write_all(data).await?;
        self.io.flush().await
    }

    pub async fn read_head(&mut self) -> Result<(Vec<u8>, Vec<u8>), HeadError> {
        let mut buf = Vec::new();
        let mut chunk = [0; 4096];
        loop {
            let n = self.io.read(&mut chunk).await.map_err(HeadError::Io)?;
            if n == 0 {
                return Err(HeadError::Io(ErrorKind::UnexpectedEof.into()));
            }
            let from = buf.len().saturating_sub(3);
            buf.extend_from_slice(&chunk[..n]);
            let end = http::end(&buf[from..]).map(|i| i + from);
            if end.unwrap_or(buf.len()) > http::MAX_HEAD {
                return Err(HeadError::TooLarge);
            }
            if let Some(end) = end {
                let rest = buf.split_off(end);
                return Ok((buf, rest));
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/core/unit/stream.rs"]
mod tests;
