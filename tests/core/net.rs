//! Networking tests over real sockets.

#[path = "net/basic.rs"]
mod basic;
#[path = "net/common.rs"]
mod common;
#[path = "net/connect.rs"]
mod connect;
#[path = "net/heartbeat.rs"]
mod heartbeat;
#[path = "net/limits.rs"]
mod limits;
#[path = "net/raw.rs"]
mod raw;
#[path = "net/shutdown.rs"]
mod shutdown;
#[path = "net/tls.rs"]
mod tls;
