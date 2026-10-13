//! Shared helpers: event pumping and a raw TCP peer.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;
use ws_core::proto::frame;
use ws_core::*;

pub const WAIT: Duration = Duration::from_secs(10);

pub fn listen(hub: &Hub) -> Server {
    hub.listen(ServerOptions { host: "127.0.0.1".into(), ..Default::default() }).unwrap()
}

pub fn url(server: &Server) -> String {
    format!("ws://127.0.0.1:{}/path?q=1", server.addr().port())
}

/// The next event, answering upgrade requests with `decide`.
pub fn next(hub: &Hub, decide: &dyn Fn(&Request) -> Reply) -> Event {
    loop {
        match hub.recv(Some(WAIT)).expect("timed out waiting for an event") {
            Event::Request { req, reply, .. } => reply.send(decide(&req)).unwrap(),
            e => return e,
        }
    }
}

pub fn accept(_: &Request) -> Reply {
    Reply::Accept(None)
}

pub fn describe(e: &Event) -> String {
    match e {
        Event::Open { id } => format!("open {id}"),
        Event::Request { .. } => "request".into(),
        Event::Connection { conn, .. } => format!("connection {}", conn.id()),
        Event::Message { id, data, binary } => format!("message {id} {binary} {}", String::from_utf8_lossy(data)),
        Event::Ping { id, .. } => format!("ping {id}"),
        Event::Pong { id, .. } => format!("pong {id}"),
        Event::Error { id, error } => format!("error {id} {error}"),
        Event::Close { id, code, reason } => format!("close {id} {code} {reason}"),
    }
}

/// Connects a client and returns (client, server-side connection).
pub fn pair(hub: &Hub, server: &Server, opts: ClientOptions) -> (Conn, Conn) {
    let client = hub.connect(&url(server), opts).unwrap();
    let mut peer = None;
    let mut opened = false;
    while peer.is_none() || !opened {
        match next(hub, &accept) {
            Event::Connection { conn, req, .. } => {
                assert_eq!(req.path, "/path?q=1");
                peer = Some(conn);
            }
            Event::Open { id } => {
                assert_eq!(id, client.id());
                opened = true;
            }
            e => panic!("unexpected {}", describe(&e)),
        }
    }
    (client, peer.unwrap())
}

/// A hand-rolled client for sending what a correct one never would.
pub struct Raw(pub TcpStream);

impl Raw {
    pub fn connect(server: &Server) -> Raw {
        let s = TcpStream::connect(server.addr()).unwrap();
        s.set_read_timeout(Some(WAIT)).unwrap();
        Raw(s)
    }

    pub fn request(&mut self, head: &str) -> String {
        self.0.write_all(head.as_bytes()).unwrap();
        let mut out = Vec::new();
        let mut b = [0u8; 1];
        while !out.ends_with(b"\r\n\r\n") {
            assert_eq!(self.0.read(&mut b).unwrap(), 1, "connection closed mid-response");
            out.push(b[0]);
        }
        String::from_utf8(out).unwrap()
    }

    pub fn upgrade(server: &Server, hub: &Hub) -> Raw {
        let mut raw = Raw::connect(server);
        raw.0.write_all(UPGRADE.as_bytes()).unwrap();
        assert!(matches!(next(hub, &accept), Event::Connection { .. }));
        let head = raw.request("");
        assert!(head.starts_with("HTTP/1.1 101 "), "{head}");
        raw
    }

    pub fn send(&mut self, op: u8, payload: &[u8]) {
        self.0.write_all(&frame::encode(op, payload, Some([9, 8, 7, 6]))).unwrap();
    }

    pub fn frame(&mut self) -> (u8, Vec<u8>) {
        let mut h = [0u8; 2];
        self.0.read_exact(&mut h).unwrap();
        assert_eq!(h[1] & 0x80, 0, "server frames are unmasked");
        let len = match h[1] & 0x7F {
            126 => {
                let mut l = [0u8; 2];
                self.0.read_exact(&mut l).unwrap();
                u16::from_be_bytes(l) as usize
            }
            127 => {
                let mut l = [0u8; 8];
                self.0.read_exact(&mut l).unwrap();
                u64::from_be_bytes(l) as usize
            }
            n => n as usize,
        };
        let mut p = vec![0u8; len];
        self.0.read_exact(&mut p).unwrap();
        (h[0] & 0x0F, p)
    }

    /// Reads until the server closes the TCP connection.
    /// Reads until the server closes the TCP connection.
    pub fn eof(&mut self) {
        let mut rest = Vec::new();
        self.0.read_to_end(&mut rest).unwrap();
    }
}

pub const UPGRADE: &str = "GET /raw HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n";
