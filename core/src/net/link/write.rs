//! The writer thread: drains the queue onto the socket.

use super::Link;
use super::send::Item;
use crate::{Event, lock};
use std::io::{ErrorKind, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::Ordering::Relaxed;

impl Link {
    pub(super) fn write_loop(&self, mut tcp: TcpStream) {
        loop {
            let items: Vec<Item> = {
                let mut q = lock(&self.queue);
                while q.items.is_empty() && !q.done {
                    q = self.ready.wait(q).unwrap_or_else(|e| e.into_inner());
                }
                if q.done {
                    return;
                }
                q.items.drain(..).collect()
            };
            for item in items {
                match item {
                    Item::Data(d) => {
                        if !self.write_out(&mut tcp, &d) {
                            return self.end();
                        }
                    }
                    Item::Shutdown => {
                        let _ = tcp.shutdown(Shutdown::Write);
                    }
                }
            }
        }
    }

    /// Writes `data` piece by piece; `false` once the socket failed or the
    /// connection ended. The socket's write timeout bounds each attempt, so
    /// a peer that stopped reading cannot hold this thread after a close.
    fn write_out(&self, tcp: &mut TcpStream, data: &[u8]) -> bool {
        let mut at = 0;
        while at < data.len() {
            match tcp.write(&data[at..]) {
                Ok(0) => return false,
                Ok(n) => {
                    at += n;
                    self.progress(n);
                }
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted) => {
                    if lock(&self.queue).done {
                        return false;
                    }
                }
                Err(_) => return false,
            }
        }
        true
    }

    /// Accounts `n` written bytes: wakes `flush_close`, records the peer as
    /// alive while a backlog remains (room for more means it acknowledged
    /// data), and reports `drain` once a refused send would now fit.
    pub(super) fn progress(&self, n: usize) {
        let drain = {
            let mut q = lock(&self.queue);
            let left = self.buffered.fetch_sub(n, Relaxed) - n;
            if left > 0 {
                let ms = u64::try_from(self.born.elapsed().as_millis()).unwrap_or(u64::MAX);
                self.acked.store(ms, Relaxed);
            }
            self.drained.notify_all();
            let drain = q.drain.is_some_and(|len| left == 0 || left.saturating_add(len) <= self.buffer);
            if drain {
                q.drain = None;
            }
            drain
        };
        // Emitting takes `flags`, which must not be taken under `queue`.
        if drain {
            self.emit(Event::Drain { id: self.id });
        }
    }
}
