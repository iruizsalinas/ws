//! Sockets and threads: dialing, accepting, handshakes and live links.

pub mod client;
mod dial;
pub(crate) mod link;
mod serve;
pub mod server;
mod stream;
