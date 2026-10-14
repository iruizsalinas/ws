//! Reader and writer futures share one runtime task and one TLS transport.
use super::budget::Budget;
use super::heartbeat::Alive;
use super::{Io, Link, State};
use crate::net::{stream::Stream, timer::Timer};
use crate::proto::reader::Reader;
use crate::{Event, lock};
use bytes::BufMut;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncRead, AsyncReadExt};

const READ_CHUNK: usize = 16 * 1024;

async fn read_chunk(stream: &mut (impl AsyncRead + Unpin), input: &mut Vec<u8>) -> std::io::Result<usize> {
    input.reserve(READ_CHUNK);
    // Retained capacity must not increase one read's work.
    stream.read_buf(&mut input.limit(READ_CHUNK)).await
}

impl Link {
    pub async fn start(self: &Arc<Self>, stream: Stream, rest: Vec<u8>, protocol: Option<String>, open: Event) {
        let _lease = self.lease();
        if self.heartbeat.is_some() {
            stream.activity.bind(self.born, self.written.clone(), self.received.clone());
        }
        let _ = self.io.set(Io::new(stream.tcp));
        let _ = self.protocol.set(protocol);
        let sent = {
            let mut f = lock(&self.flags);
            if f.state != State::Connecting {
                if let Some(io) = self.io.get() {
                    io.shutdown();
                }
                return;
            }
            f.state = State::Open;
            self.events.send(open).is_ok()
        };
        if !sent || self.events.closed() {
            return self.terminate();
        }
        let (reader, writer) = tokio::io::split(stream.io);
        tokio::join!(self.read_loop(reader, rest), self.write_loop(writer, stream.flush, stream.chunk));
    }

    async fn read_loop(&self, mut stream: impl AsyncRead + Unpin, rest: Vec<u8>) {
        let mut reader = Reader::new(self.server, self.max);
        reader.push(&rest);
        let mut ok = true;
        // Allocate discard storage only after frames may no longer be parsed.
        let mut discard = Vec::new();
        let mut alive = Alive::new();
        let stopped = self.stopped.wait();
        let closed = self.events.wait_closed();
        tokio::pin!(stopped, closed);
        let mut timer = Timer::default();
        let mut budget = Budget::default();
        loop {
            if self.events.closed() {
                self.terminate();
                break;
            }
            if ok {
                ok = self.dispatch(&mut reader);
            }
            let (state, deadline) = {
                let flags = lock(&self.flags);
                (flags.state, flags.deadline)
            };
            if state == State::Closed || deadline.is_some_and(|at| Instant::now() >= at) {
                break;
            }
            if !self.wait_inbox(state) {
                tokio::select! {
                    biased;
                    _ = &mut stopped => break,
                    _ = &mut closed => { self.terminate(); break; }
                    _ = self.changed.notified() => {}
                    _ = self.consumed.notified() => {}
                }
                continue;
            }
            timer.set(deadline.or_else(|| self.heartbeat_deadline(&alive)));
            let input = if ok {
                reader.input()
            } else {
                discard.clear();
                &mut discard
            };
            let result = tokio::select! {
                biased;
                _ = &mut stopped => break,
                _ = &mut closed => { self.terminate(); break; }
                _ = self.changed.notified() => continue,
                result = read_chunk(&mut stream, input) => result,
                _ = timer.wait() => {
                    if !self.heartbeat(&mut alive) { break; }
                    continue;
                }
            };
            match result {
                Ok(0) => break,
                Ok(n) => {
                    alive.seen = Instant::now();
                    budget.progress(n).await;
                }
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::InvalidData {
                        self.fail(&format!("TLS error: {e}"));
                    }
                    break;
                }
            }
        }
        if ok {
            self.dispatch(&mut reader);
        }
        self.flush_close().await;
        self.end();
    }
}

#[cfg(test)]
#[path = "../../../../tests/core/unit/read.rs"]
mod tests;
