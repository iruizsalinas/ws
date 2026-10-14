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

#[test]
fn dropping_a_hub_cancels_a_writer_blocked_by_a_peer_that_does_not_read() {
    use std::io::Read;
    let hub = Hub::default();
    let server = hub.listen(ServerOptions { host: "127.0.0.1".into(), heartbeat: None, ..Default::default() }).unwrap();
    let (mut raw, conn) = Raw::open(&server, &hub);
    conn.send(&vec![7; 32 << 20], true).unwrap();
    drop(hub);
    let until = Instant::now() + Duration::from_secs(3);
    while conn.state() != State::Closed && Instant::now() < until {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(conn.state(), State::Closed, "a blocked write retained the closed hub");
    raw.0.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    let mut rest = Vec::new();
    raw.0.read_to_end(&mut rest).unwrap();
}
