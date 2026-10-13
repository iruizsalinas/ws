//! Malformed handshakes and frames from a raw TCP peer.

use super::common::*;
use std::io::Write;
use ws_core::proto::frame::{self, CLOSE, PING, PONG, TEXT};
use ws_core::*;

#[test]
fn bad_requests_get_bodiless_statuses() {
    let hub = Hub::default();
    let server = listen(&hub);
    let cases = [
        ("GET / HTTP/1.1\r\nHost: a\r\n\r\n", 426),
        ("POST / HTTP/1.1\r\nHost: a\r\n\r\n", 405),
        ("garbage\r\n\r\n", 400),
        (&UPGRADE.replace("Version: 13", "Version: 7"), 426),
        (&UPGRADE.replace("dGhlIHNhbXBsZSBub25jZQ==", "bad"), 400),
    ];
    for (request, status) in cases {
        let mut raw = Raw::connect(&server);
        let head = raw.request(request);
        assert!(head.starts_with(&format!("HTTP/1.1 {status} ")), "{head}");
        assert!(head.contains("Content-Length: 0\r\n"), "{head}");
        raw.eof();
    }
    // Oversized heads are refused before they are buffered without bound.
    let mut raw = Raw::connect(&server);
    let huge = format!("GET / HTTP/1.1\r\nX: {}\r\n\r\n", "a".repeat(20_000));
    assert!(raw.request(&huge).starts_with("HTTP/1.1 431 "));
    // Requests that the application rejects get its status.
    let mut raw = Raw::connect(&server);
    raw.0.write_all(UPGRADE.as_bytes()).unwrap();
    match hub.recv(Some(WAIT)).unwrap() {
        Event::Request { req, reply, .. } => {
            assert_eq!(req.path, "/raw");
            assert!(req.headers.iter().any(|(n, v)| n == "host" && v == "localhost"));
            reply.send(Reply::Reject(401)).unwrap();
        }
        e => panic!("unexpected {}", describe(&e)),
    }
    assert!(raw.request("").starts_with("HTTP/1.1 401 Unauthorized\r\n"));
}

#[test]
fn protocol_violations_close_with_codes() {
    let hub = Hub::default();
    let server = listen(&hub);
    let unmasked = |raw: &mut Raw| raw.0.write_all(&frame::encode(TEXT, b"x", None)).unwrap();
    let bad_utf8 = |raw: &mut Raw| raw.send(TEXT, &[0xFF]);
    let bad_code = |raw: &mut Raw| raw.send(CLOSE, &[0x03, 0xED]);
    let reserved = |raw: &mut Raw| raw.0.write_all(&[0xC1, 0x80, 0, 0, 0, 0]).unwrap();
    type Violation<'a> = &'a dyn Fn(&mut Raw);
    let cases: [(Violation, u16); 4] = [(&unmasked, 1002), (&bad_utf8, 1007), (&bad_code, 1002), (&reserved, 1002)];
    for (violate, code) in cases {
        let mut raw = Raw::upgrade(&server, &hub);
        violate(&mut raw);
        let (op, payload) = raw.frame();
        assert_eq!((op, u16::from_be_bytes([payload[0], payload[1]])), (CLOSE, code));
        raw.eof();
        drop(raw);
        assert!(matches!(next(&hub, &accept), Event::Error { .. }));
        assert!(matches!(next(&hub, &accept), Event::Close { code: c, .. } if c == code));
    }
}

#[test]
fn raw_fragments_ping_and_close_echo() {
    let hub = Hub::default();
    let server = listen(&hub);
    let mut raw = Raw::upgrade(&server, &hub);
    let mut first = frame::encode(TEXT, b"frag", Some([1, 2, 3, 4]));
    first[0] &= 0x7F;
    raw.0.write_all(&first).unwrap();
    raw.send(PING, b"mid");
    raw.send(0x0, b"mented");
    assert_eq!(raw.frame(), (PONG, b"mid".to_vec()));
    assert!(matches!(next(&hub, &accept), Event::Ping { .. }));
    match next(&hub, &accept) {
        Event::Message { data, binary: false, .. } => assert_eq!(data, b"fragmented"),
        e => panic!("unexpected {}", describe(&e)),
    }
    raw.send(CLOSE, &frame::close_payload(1001, "away"));
    let (op, payload) = raw.frame();
    assert_eq!((op, payload), (CLOSE, vec![0x03, 0xE9]));
    raw.eof(); // the server closes TCP first
    drop(raw);
    match next(&hub, &accept) {
        Event::Close { code, reason, .. } => assert_eq!((code, reason.as_str()), (1001, "away")),
        e => panic!("unexpected {}", describe(&e)),
    }
}

#[test]
fn dropped_tcp_is_abnormal() {
    let hub = Hub::default();
    let server = listen(&hub);
    let raw = Raw::upgrade(&server, &hub);
    drop(raw);
    assert!(matches!(next(&hub, &accept), Event::Close { code: 1006, .. }));
}

#[test]
fn invalid_upgrade_decisions_are_rejected() {
    for decision in [
        Reply::Accept(Some("chat\r\nX-Injected: yes".into())),
        Reply::Accept(Some("unoffered".into())),
        Reply::Accept(Some(String::new())),
        Reply::Reject(101),
    ] {
        let hub = Hub::default();
        let server = listen(&hub);
        let mut raw = Raw::connect(&server);
        raw.0.write_all(UPGRADE.as_bytes()).unwrap();
        let Event::Request { reply, .. } = hub.recv(Some(WAIT)).unwrap() else { panic!("expected request") };
        reply.send(decision).unwrap();
        let head = raw.request("");
        assert!(head.starts_with("HTTP/1.1 500 "), "{head}");
        assert!(!head.contains("X-Injected"));
    }
}

#[test]
fn frames_after_close_are_ignored() {
    let hub = Hub::default();
    let server = listen(&hub);
    let mut raw = Raw::upgrade(&server, &hub);
    let mut bytes = frame::encode(CLOSE, &frame::close_payload(1000, "bye"), Some([1; 4]));
    bytes.extend(frame::encode(PING, b"ignored", Some([1; 4])));
    bytes.extend([0xff, 0xff]);
    raw.0.write_all(&bytes).unwrap();
    assert_eq!(raw.frame().0, CLOSE);
    raw.eof();
    drop(raw);
    assert!(matches!(next(&hub, &accept), Event::Close { code: 1000, .. }));
}

#[test]
fn ping_is_answered_while_waiting_for_peer_close() {
    let hub = Hub::default();
    let server = listen(&hub);
    let mut raw = Raw::connect(&server);
    raw.0.write_all(UPGRADE.as_bytes()).unwrap();
    let Event::Connection { conn, .. } = next(&hub, &accept) else { panic!("expected connection") };
    assert!(raw.request("").starts_with("HTTP/1.1 101 "));
    conn.close(1000, "").unwrap();
    assert_eq!(raw.frame().0, CLOSE);
    raw.send(PING, b"still here");
    assert_eq!(raw.frame(), (PONG, b"still here".to_vec()));
    raw.send(CLOSE, &frame::close_payload(1000, ""));
    raw.eof();
}

#[test]
fn peer_half_close_does_not_discard_close_reply() {
    let hub = Hub::default();
    let server = listen(&hub);
    let mut raw = Raw::connect(&server);
    raw.0.write_all(UPGRADE.as_bytes()).unwrap();
    let Event::Connection { conn, .. } = next(&hub, &accept) else { panic!("expected connection") };
    assert!(raw.request("").starts_with("HTTP/1.1 101 "));
    // Hold the writer in a large frame while the reader handles Close and EOF.
    let data = vec![7; 8 * 1024 * 1024];
    conn.send(&data, true).unwrap();
    raw.send(CLOSE, &frame::close_payload(1000, ""));
    raw.0.shutdown(std::net::Shutdown::Write).unwrap();
    assert_eq!(raw.frame(), (frame::BINARY, data));
    assert_eq!(raw.frame(), (CLOSE, frame::close_payload(1000, "")));
    raw.eof();
}

#[test]
fn full_inbox_does_not_block_close_reply() {
    let hub = Hub::default();
    let server = listen(&hub);
    let mut raw = Raw::connect(&server);
    raw.0.write_all(UPGRADE.as_bytes()).unwrap();
    let Event::Connection { conn, .. } = next(&hub, &accept) else { panic!("expected connection") };
    assert!(raw.request("").starts_with("HTTP/1.1 101 "));
    raw.send(frame::BINARY, &vec![0; MAX_MESSAGE]);
    assert!(matches!(next(&hub, &accept), Event::Message { .. }));
    // Intentionally do not acknowledge the message: the inbox remains full.
    conn.close(1000, "").unwrap();
    assert_eq!(raw.frame().0, CLOSE);
    raw.send(CLOSE, &frame::close_payload(1000, ""));
    raw.eof();
    drop(raw);
    assert!(matches!(next(&hub, &accept), Event::Close { code: 1000, .. }));
}
