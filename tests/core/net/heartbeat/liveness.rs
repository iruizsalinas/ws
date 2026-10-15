use super::*;

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
        // Confirm the server consumed the traffic before checking for a ping.
        match next(&hub, &accept) {
            Event::Message { id, data, binary } => {
                assert_eq!(id, conn.id());
                assert_eq!(data, b"busy");
                assert!(!binary);
            }
            e => panic!("unexpected {}", describe(&e)),
        }
        assert!(raw.quiet(Duration::from_millis(50)), "unexpected socket activity while the peer was sending");
    }
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
