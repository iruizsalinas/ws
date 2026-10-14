//! Async writes yield between bounded chunks; no lock crosses an await.
use super::budget::Budget;
use super::{Link, heartbeat::Sent, send::Item};
use crate::{Event, lock};
use std::collections::VecDeque;
use std::io;
use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;
use std::time::Duration;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::Notify;

impl Link {
    pub(super) async fn write_loop(&self, mut stream: impl AsyncWrite + Unpin, flush: Arc<Notify>, chunk: usize) {
        // Keep cancellation registrations for the lifetime of the writer,
        // including while a write, flush or shutdown is waiting on the socket.
        tokio::select! {
            biased;
            _ = self.stopped.wait() => (),
            _ = self.events.wait_closed() => self.terminate(),
            result = self.write_pending(&mut stream, &flush, chunk) => {
                if result.is_err() { self.end(); }
            }
        }
    }

    async fn write_pending(
        &self,
        stream: &mut (impl AsyncWrite + Unpin),
        flush: &Notify,
        chunk: usize,
    ) -> io::Result<()> {
        let mut items = VecDeque::new();
        let mut budget = Budget::default();
        loop {
            let pong = {
                let mut q = lock(&self.queue);
                if q.done {
                    return Ok(());
                }
                // Exchange two reusable empty/nonempty buffers, without
                // allocating a temporary Vec on every batch.
                std::mem::swap(&mut items, &mut q.items);
                q.pong.is_some()
            };
            if items.is_empty() && !pong {
                tokio::select! {
                    _ = self.ready.notified() => {}
                    _ = flush.notified() => { stream.flush().await?; }
                }
                continue;
            }
            if pong {
                self.flush_pong();
            }
            while let Some(item) = items.pop_front() {
                match item {
                    Item::Data(data) => self.write_out(stream, &data, None, &mut budget, chunk).await?,
                    Item::Heartbeat(data, sent) => {
                        self.write_out(stream, &data, Some(&sent), &mut budget, chunk).await?
                    }
                    Item::Shutdown => {
                        stream.shutdown().await?;
                        lock(&self.queue).shutdown = false;
                        self.drained.notify_one();
                    }
                }
            }
        }
    }

    async fn write_out(
        &self,
        stream: &mut (impl AsyncWrite + Unpin),
        data: &[u8],
        sent: Option<&Sent>,
        budget: &mut Budget,
        chunk: usize,
    ) -> io::Result<()> {
        let mut at = 0;
        while at < data.len() {
            let n = stream.write(&data[at..data.len().min(at + chunk)]).await?;
            if n == 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }
            stream.flush().await?;
            at += n;
            // A write or TLS flush may wait arbitrarily long. Start the probe's
            // clock at actual TCP progress, not before entering that await.
            let transmitted = sent.map(|_| self.born + Duration::from_nanos(self.written.load(Relaxed)));
            self.progress(n);
            if at == data.len()
                && let Some(sent) = sent
            {
                *lock(sent) = transmitted;
                self.changed.notify_one();
            }
            budget.progress(n).await;
        }
        Ok(())
    }

    pub(super) fn progress(&self, n: usize) {
        let drain = {
            let mut q = lock(&self.queue);
            let left = self.buffered.fetch_sub(n, Relaxed) - n;
            self.drained.notify_one();
            let drain = q.drain.is_some_and(|len| left == 0 || left.saturating_add(len) <= self.buffer);
            if drain {
                q.drain = None;
            }
            drain
        };
        // emit() takes flags; never call it while holding queue (lock order).
        if drain {
            self.emit(Event::Drain { id: self.id });
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/core/unit/write.rs"]
mod tests;
