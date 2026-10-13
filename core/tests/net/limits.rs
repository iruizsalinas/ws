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
