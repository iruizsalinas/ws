//! Heartbeat: quiet peers are pinged, silent ones dropped, live ones kept.

use super::common::*;
use std::time::{Duration, Instant};
use ws_core::proto::frame::{PING, PONG, TEXT};
use ws_core::*;

// Real sockets need room for scheduler delays on shared CI runners. Exact
// heartbeat boundaries are covered separately by the unit tests.
const IV: Duration = Duration::from_secs(1);

fn server(hub: &Hub, heartbeat: Option<Duration>) -> Server {
    hub.listen(ServerOptions { host: "127.0.0.1".into(), heartbeat, ..Default::default() }).unwrap()
}

/// The error and close events of a connection dropped by the heartbeat.
fn expect_dropped(hub: &Hub, conn: &Conn) {
    match next(hub, &accept) {
        Event::Error { id, error } => assert_eq!((id, error.as_str()), (conn.id(), "heartbeat timed out")),
        e => panic!("unexpected {}", describe(&e)),
    }
    assert!(matches!(next(hub, &accept), Event::Close { code: 1006, .. }));
    assert_eq!(conn.state(), State::Closed);
}

#[path = "heartbeat/client.rs"]
mod client;
#[path = "heartbeat/liveness.rs"]
mod liveness;
