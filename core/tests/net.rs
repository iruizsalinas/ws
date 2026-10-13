//! Networking tests over real sockets.

#[path = "net/basic.rs"]
mod basic;
#[path = "net/common.rs"]
mod common;
#[path = "net/connect.rs"]
mod connect;
#[path = "net/raw.rs"]
mod raw;
#[path = "net/tls.rs"]
mod tls;
