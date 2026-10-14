//! Track transport progress even while TLS is waiting for a complete record.
use std::io::{self, IoSlice};
use std::pin::Pin;
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicU64, Ordering::Relaxed},
};
use std::task::{Context, Poll};
use std::time::Instant;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

struct Target {
    born: Instant,
    written: Arc<AtomicU64>,
    received: Arc<AtomicU64>,
}

#[derive(Default)]
pub(crate) struct Activity(OnceLock<Target>);

impl Activity {
    pub fn bind(&self, born: Instant, written: Arc<AtomicU64>, received: Arc<AtomicU64>) {
        let _ = self.0.set(Target { born, written, received });
    }
    fn progress(&self, inbound: bool) {
        if let Some(target) = self.0.get() {
            let ns = u64::try_from(target.born.elapsed().as_nanos()).unwrap_or(u64::MAX);
            if inbound {
                target.received.store(ns.saturating_add(1), Relaxed);
            } else {
                target.written.store(ns, Relaxed);
            }
        }
    }
}

pub(super) struct Observed<T> {
    pub io: T,
    pub activity: Arc<Activity>,
}

impl<T: AsyncRead + Unpin> AsyncRead for Observed<T> {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let before = buf.filled().len();
        let result = Pin::new(&mut this.io).poll_read(cx, buf);
        if matches!(result, Poll::Ready(Ok(()))) && buf.filled().len() > before {
            this.activity.progress(true);
        }
        result
    }
}

impl<T: AsyncWrite + Unpin> AsyncWrite for Observed<T> {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.io).poll_write(cx, buf);
        if matches!(result, Poll::Ready(Ok(n)) if n > 0) {
            this.activity.progress(false);
        }
        result
    }
    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.io).poll_write_vectored(cx, bufs);
        if matches!(result, Poll::Ready(Ok(n)) if n > 0) {
            this.activity.progress(false);
        }
        result
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
