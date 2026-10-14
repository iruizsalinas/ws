//! TLS reads can queue post-handshake replies. Wake the sole writer to flush
//! them promptly, even on idle connections with heartbeats disabled.
use super::activity::Observed;
use std::io::{self, IoSlice, Write};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio::sync::Notify;

pub(super) struct Tls {
    pub io: tokio_rustls::TlsStream<Observed<TcpStream>>,
    pub flush: Arc<Notify>,
}

impl AsyncRead for Tls {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.io).poll_read(cx, buf);
        // rustls deliberately defers KeyUpdate replies until its plaintext
        // writer is used. An empty write exposes that reply without creating
        // application bytes or performing socket I/O on the reader's path.
        if !matches!(result, Poll::Ready(Err(_))) {
            let exposed = match &mut this.io {
                tokio_rustls::TlsStream::Client(stream) => stream.get_mut().1.writer().write(&[]),
                tokio_rustls::TlsStream::Server(stream) => stream.get_mut().1.writer().write(&[]),
            };
            if let Err(error) = exposed {
                return Poll::Ready(Err(error));
            }
        }
        if this.io.get_ref().1.wants_write() {
            this.flush.notify_one();
        }
        result
    }
}

impl AsyncWrite for Tls {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().io).poll_write(cx, buf)
    }
    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().io).poll_write_vectored(cx, bufs)
    }
    fn is_write_vectored(&self) -> bool {
        self.io.is_write_vectored()
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().io).poll_flush(cx)
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().io).poll_shutdown(cx)
    }
}
