//! Connection admission and capacity recovery.

use super::common::*;
use std::time::{Duration, Instant};
use ws_core::proto::frame;
use ws_core::*;

#[test]
fn admission_limit_includes_open_connections_and_recovers_after_close() {
    use std::io::{Read, Write};
    let hub = Hub::default();
    let server =
        hub.listen(ServerOptions { host: "127.0.0.1".into(), max_connections: 1, ..Default::default() }).unwrap();
    let mut first = Raw::connect(&server);
    first.0.write_all(UPGRADE.as_bytes()).unwrap();
    let Event::Connection { conn, .. } = next(&hub, &accept) else { panic!("expected connection") };
    assert!(first.request("").starts_with("HTTP/1.1 101 "));
    let mut excess = Raw::connect(&server);
    let mut buf = [0; 1];
    match excess.0.read(&mut buf) {
        Ok(0) => {}
        Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => {}
        other => panic!("excess connection was not dropped: {other:?}"),
    }
    first.send(frame::CLOSE, &frame::close_payload(1000, ""));
    assert_eq!(first.frame().0, frame::CLOSE);
    // Half-close so the reader exits without waiting for the close timeout.
    first.0.shutdown(std::net::Shutdown::Write).unwrap();
    first.eof();
    // A peer must not regain admission by closing faster than the app polls.
    let mut churn = Raw::connect(&server);
    match churn.0.read(&mut buf) {
        Ok(0) => {}
        Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => {}
        other => panic!("undrained close event did not retain capacity: {other:?}"),
    }
    assert!(matches!(next(&hub, &accept), Event::Close { .. }));
    // The final event can precede worker teardown by a scheduling quantum.
    let until = Instant::now() + WAIT;
    loop {
        let mut retry = Raw::connect(&server);
        let _ = retry.0.write_all(UPGRADE.as_bytes());
        if let Some(event) = hub.recv(Some(Duration::from_millis(50))) {
            let Event::Request { reply, .. } = event else { panic!("expected request") };
            reply.send(Reply::Accept(None)).unwrap();
            let Event::Connection { conn, .. } = next(&hub, &accept) else { panic!("expected connection") };
            assert!(retry.request("").starts_with("HTTP/1.1 101 "));
            conn.terminate();
            break;
        }
        assert!(Instant::now() < until, "admission capacity was leaked");
    }
    assert_eq!(conn.state(), State::Closed, "keeping a closed handle must not retain capacity");
}

#[test]
fn drain_follows_a_refused_send_once_there_is_room() {
    let hub = Hub::default();
    let opts = ServerOptions { host: "127.0.0.1".into(), max_buffer: 1, keepalive: None, ..Default::default() };
    let server = hub.listen(opts).unwrap();
    let (mut raw, conn) = Raw::open(&server, &hub);
    let big = vec![7u8; 32 << 20];
    conn.send(&big, true).unwrap();
    // Refused while the first message is still leaving the process.
    assert_eq!(conn.send(b"x", true), Err("send buffer is full"));
    // Room may come from kernel buffers or from the peer reading; read on
    // another thread so the test does not depend on their size.
    // The thread hands the socket back, so the connection outlives it.
    let reader = std::thread::spawn(move || (raw.frame(), raw.frame(), raw));
    match next(&hub, &accept) {
        Event::Drain { id } => assert_eq!(id, conn.id()),
        e => panic!("unexpected {}", describe(&e)),
    }
    conn.send(b"after", true).unwrap();
    let (first, second, _raw) = reader.join().unwrap();
    assert_eq!(first, (frame::BINARY, big), "the refused frame was not queued");
    assert_eq!(second, (frame::BINARY, b"after".to_vec()));
    if let Some(e) = hub.recv(Some(Duration::from_millis(100))) {
        panic!("unexpected {} after the drain", describe(&e));
    }
    conn.terminate();
}
