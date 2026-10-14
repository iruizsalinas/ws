//! What the asynchronous transport reports to the application.

use crate::{Conn, Headers, Id, ReplySender};
use std::net::SocketAddr;

/// An upgrade request awaiting a decision.
#[derive(Clone, Debug)]
pub struct Request {
    pub path: String,
    pub headers: Headers,
    pub protocols: Vec<String>,
    pub addr: SocketAddr,
}

pub enum Reply {
    Accept(Option<String>),
    /// Rejects with this HTTP status and no body.
    Reject(u16),
}

pub enum Event {
    /// A client connection completed its handshake.
    Open {
        id: Id,
    },
    /// A server must accept or reject an upgrade.
    Request {
        server: Id,
        req: Request,
        reply: ReplySender,
    },
    /// A server accepted a connection.
    Connection {
        server: Id,
        conn: Conn,
        req: Request,
    },
    Message {
        id: Id,
        data: Vec<u8>,
        binary: bool,
    },
    Ping {
        id: Id,
        data: Vec<u8>,
    },
    Pong {
        id: Id,
        data: Vec<u8>,
    },
    Error {
        id: Id,
        error: String,
    },
    /// A send was refused for a full buffer, and there is room again.
    Drain {
        id: Id,
    },
    /// Always the last event of a connection.
    Close {
        id: Id,
        code: u16,
        reason: String,
    },
}
