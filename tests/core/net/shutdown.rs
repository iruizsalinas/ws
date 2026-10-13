//! Hub destruction must release quiet and throttled network workers.

use super::common::*;
use std::thread;
use std::time::{Duration, Instant};
use ws_core::proto::frame;
use ws_core::*;

#[test]
fn dropping_a_hub_closes_idle_and_full_connections_without_heartbeats() {
    for full in [false, true] {
        let hub = Hub::default();
        let server =
            hub.listen(ServerOptions { host: "127.0.0.1".into(), heartbeat: None, ..Default::default() }).unwrap();
        let (mut raw, conn) = Raw::open(&server, &hub);
        if full {
            raw.send(frame::BINARY, &vec![0; MAX_MESSAGE]);
            assert!(matches!(next(&hub, &accept), Event::Message { .. }));
            // Leave the inbox charged: there is no application left to drain it.
        }
        drop(hub);
        let until = Instant::now() + Duration::from_secs(3);
        while conn.state() != State::Closed && Instant::now() < until {
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(conn.state(), State::Closed, "full inbox: {full}");
        raw.eof();
    }
}
