//! Heartbeat: quiet peers are pinged, silent ones dropped, live ones kept.

use super::common::*;
use std::time::{Duration, Instant};
use ws_core::proto::frame::{PING, PONG, TEXT};
use ws_core::*;

const IV: Duration = Duration::from_millis(200);

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

#[test]
fn silent_peer_is_pinged_then_dropped() {
    let hub = Hub::default();
    let server = server(&hub, Some(IV));
    let (mut raw, conn) = Raw::open(&server, &hub);
    let start = Instant::now();
    let (op, token) = raw.frame();
    assert_eq!((op, token.len()), (PING, 8), "a heartbeat ping carries an 8-byte token");
    assert!(start.elapsed() >= IV / 2, "pinged before the peer was quiet: {:?}", start.elapsed());
    // Never answer it.
    let pinged = Instant::now();
    expect_dropped(&hub, &conn);
    let waited = pinged.elapsed();
    assert!(waited >= IV / 2 && waited < IV * 10, "dropped after {waited:?}");
    // The TCP connection is gone too (EOF or reset, not a read timeout).
    let gone = match std::io::Read::read_to_end(&mut raw.0, &mut Vec::new()) {
        Ok(_) => true,
        Err(e) => matches!(e.kind(), std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted),
    };
    assert!(gone, "socket still open after a heartbeat drop");
}

#[test]
fn answered_pings_keep_the_connection_and_stay_internal() {
    let hub = Hub::default();
    let server = server(&hub, Some(IV));
    let (mut raw, conn) = Raw::open(&server, &hub);
    // Three answered rounds outlast the two intervals a silent peer gets.
    for _ in 0..3 {
        let (op, token) = raw.frame();
        assert_eq!(op, PING);
        raw.send(PONG, &token);
    }
    assert_eq!(conn.state(), State::Open);
    // None of those pongs was reported; a pong the peer chose to send is.
    raw.send(PONG, b"app");
    match next(&hub, &accept) {
        Event::Pong { data, .. } => assert_eq!(data, b"app"),
        e => panic!("unexpected {}", describe(&e)),
    }
    conn.terminate();
}

#[test]
fn inbound_traffic_defers_pings() {
    let hub = Hub::default();
    let server = server(&hub, Some(IV));
    let (mut raw, conn) = Raw::open(&server, &hub);
    let until = Instant::now() + IV * 3;
    while Instant::now() < until {
        raw.send(TEXT, b"busy");
        std::thread::sleep(IV / 4);
    }
    assert!(raw.quiet(Duration::from_millis(10)), "pinged a peer that was sending");
    assert_eq!(conn.state(), State::Open);
    // Once the traffic stops, the quiet peer is pinged after all.
    assert_eq!(raw.frame().0, PING);
    conn.terminate();
}

#[test]
fn disabled_heartbeat_never_pings() {
    let hub = Hub::default();
    let server = server(&hub, None);
    let (mut raw, conn) = Raw::open(&server, &hub);
    assert!(raw.quiet(IV * 3));
    assert_eq!(conn.state(), State::Open);
    conn.terminate();
}

#[test]
fn a_stalled_backlog_does_not_hide_a_dead_peer() {
    let hub = Hub::default();
    let server = server(&hub, Some(IV));
    // The raw peer never reads, so the writer blocks once the kernel is full.
    let (_raw, conn) = Raw::open(&server, &hub);
    // Larger than the buffer, so it goes alone, and more than kernels hold.
    conn.send(&vec![7u8; 32 << 20], true).unwrap();
    expect_dropped(&hub, &conn);
    assert_eq!(conn.buffered(), 0);
}

#[test]
fn our_own_sends_do_not_keep_a_silent_peer_alive() {
    let hub = Hub::default();
    let server = server(&hub, Some(IV));
    // The peer's kernel accepts what we send, but it never reads or answers.
    let (_raw, conn) = Raw::open(&server, &hub);
    let until = Instant::now() + IV * 10;
    while conn.send(b"tick", false).is_ok() {
        assert!(Instant::now() < until, "a peer that never answered was kept alive by our writes");
        std::thread::sleep(IV / 4);
    }
    expect_dropped(&hub, &conn);
}

#[test]
fn client_heartbeat_pings_and_hides_its_pongs() {
    let hub = Hub::default();
    let server = server(&hub, None);
    let (client, peer) = pair(&hub, &server, ClientOptions { heartbeat: Some(IV), ..Default::default() });
    let mut pings = 0;
    while pings < 2 {
        match next(&hub, &accept) {
            // The server's application sees the client's pings...
            Event::Ping { id, data } => {
                assert_eq!((id, data.len()), (peer.id(), 8));
                pings += 1;
            }
            // ...but the client's never sees the automatic pongs.
            e => panic!("unexpected {}", describe(&e)),
        }
    }
    assert_eq!(client.state(), State::Open);
    client.terminate();
    peer.terminate();
}
