//! Blocking stream for the opening handshake, bounded by a deadline.

use crate::TICK;
use crate::proto::http;
use rustls::Connection;
use std::io::{self, ErrorKind, Read, Write};
use std::net::TcpStream;
use std::time::Instant;

pub(crate) struct Stream<'a> {
    pub tcp: TcpStream,
    pub tls: Option<Connection>,
    deadline: Instant,
    alive: &'a dyn Fn() -> bool,
}

/// Check the absolute deadline on every syscall, including those inside rustls.
struct Bounded<'a> {
    tcp: &'a mut TcpStream,
    deadline: Instant,
    alive: &'a dyn Fn() -> bool,
}

impl Bounded<'_> {
    fn remaining(&self) -> io::Result<std::time::Duration> {
        if !(self.alive)() {
            return Err(io::Error::other("connection closed before it was established"));
        }
        let left = self.deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(io::Error::new(ErrorKind::TimedOut, "handshake timed out"));
        }
        Ok(left)
    }
}

impl Read for Bounded<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.tcp.set_read_timeout(Some(self.remaining()?.min(TICK)))?;
        self.tcp.read(buf)
    }
}

impl Write for Bounded<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.tcp.set_write_timeout(Some(self.remaining()?))?;
        self.tcp.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.tcp.flush()
    }
}

impl<'a> Stream<'a> {
    pub fn new(tcp: TcpStream, tls: Option<Connection>, deadline: Instant, alive: &'a dyn Fn() -> bool) -> Self {
        Stream { tcp, tls, deadline, alive }
    }

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut io = Bounded { tcp: &mut self.tcp, deadline: self.deadline, alive: self.alive };
        loop {
            // Short reads let a cancelled or expired handshake stop promptly.
            io.remaining()?;
            let r = match &mut self.tls {
                Some(tls) => read_tls(tls, &mut io, buf),
                None => io.read(buf),
            };
            match r {
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted) => {}
                r => return r,
            }
        }
    }

    pub fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        let mut io = Bounded { tcp: &mut self.tcp, deadline: self.deadline, alive: self.alive };
        io.remaining()?;
        match &mut self.tls {
            Some(tls) => {
                tls.writer().write_all(data)?;
                flush_tls(tls, &mut io)
            }
            None => io.write_all(data),
        }
    }

    /// Reads an HTTP head; returns it with any bytes that followed.
    pub fn read_head(&mut self) -> Result<(Vec<u8>, Vec<u8>), HeadError> {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let n = self.read(&mut chunk).map_err(HeadError::Io)?;
            if n == 0 {
                return Err(HeadError::Io(ErrorKind::UnexpectedEof.into()));
            }
            // Only rescan the tail that could complete a terminator.
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

    /// Finishes the TLS handshake (a no-op for plain TCP).
    pub fn handshake(&mut self) -> io::Result<()> {
        let mut io = Bounded { tcp: &mut self.tcp, deadline: self.deadline, alive: self.alive };
        while self.tls.as_ref().is_some_and(|t| t.is_handshaking()) {
            io.remaining()?;
            let tls = self.tls.as_mut().unwrap_or_else(|| unreachable!());
            match tls.complete_io(&mut io) {
                Ok((0, 0)) => return Err(ErrorKind::UnexpectedEof.into()),
                Ok(_) => {}
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

fn read_tls(tls: &mut Connection, tcp: &mut (impl Read + Write), buf: &mut [u8]) -> io::Result<usize> {
    loop {
        match tls.reader().read(buf) {
            Err(e) if e.kind() == ErrorKind::WouldBlock => {}
            r => return r,
        }
        let n = tls.read_tls(tcp)?;
        let processed = tls.process_new_packets();
        flush_tls(tls, tcp)?;
        processed.map_err(|e| io::Error::new(ErrorKind::InvalidData, e))?;
        if n == 0 {
            return tls.reader().read(buf);
        }
    }
}

pub(crate) fn flush_tls(tls: &mut Connection, tcp: &mut impl Write) -> io::Result<()> {
    while tls.wants_write() {
        tls.write_tls(tcp)?;
    }
    Ok(())
}

pub(crate) enum HeadError {
    Io(io::Error),
    TooLarge,
}

// Included here to exercise deadlines without exposing handshake internals.
#[cfg(test)]
#[path = "../../../tests/core/unit/stream.rs"]
mod tests;
