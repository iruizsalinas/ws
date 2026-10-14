//! Connection and listener settings.
use crate::{HEARTBEAT, Headers, MAX_BUFFER, MAX_MESSAGE};
use rustls::{ClientConfig, ServerConfig};
use std::sync::Arc;
use std::time::Duration;

pub struct Client {
    pub protocols: Vec<String>,
    pub headers: Headers,
    /// WSS verification settings; None trusts Mozilla's root certificates.
    pub tls: Option<Arc<ClientConfig>>,
    /// Absolute budget for DNS, connecting, TLS and HTTP upgrading together.
    pub timeout: Duration,
    /// Maximum assembled inbound message size, including fragments.
    pub max_message: usize,
    /// Outbound WebSocket bytes; one larger send is allowed when empty.
    pub max_buffer: usize,
    /// Quiet-peer probe interval; None disables heartbeats.
    pub heartbeat: Option<Duration>,
}

impl Default for Client {
    fn default() -> Self {
        Self {
            protocols: vec![],
            headers: vec![],
            tls: None,
            timeout: Duration::from_secs(10),
            max_message: MAX_MESSAGE,
            max_buffer: MAX_BUFFER,
            heartbeat: Some(HEARTBEAT),
        }
    }
}

pub struct Server {
    pub host: String,
    pub port: u16,
    /// Certificate and key configuration; None listens without TLS.
    pub tls: Option<Arc<ServerConfig>>,
    /// Maximum assembled inbound message size, including fragments.
    pub max_message: usize,
    /// Outbound WebSocket bytes; one larger send is allowed when empty.
    pub max_buffer: usize,
    /// Quiet-peer probe interval; None disables heartbeats.
    pub heartbeat: Option<Duration>,
    /// Includes handshakes and undrained events; unlimited by default.
    pub max_connections: usize,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".into(),
            port: 0,
            tls: None,
            max_message: MAX_MESSAGE,
            max_buffer: MAX_BUFFER,
            heartbeat: Some(HEARTBEAT),
            max_connections: usize::MAX,
        }
    }
}
