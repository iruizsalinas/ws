//! wss:// with a private CA.

use super::common::*;
use ws_core::*;

#[test]
fn tls() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures/tls/");
    let hub = Hub::default();
    let config = tls::server(&format!("{dir}server.pem"), &format!("{dir}server-key.pem")).unwrap();
    let server =
        hub.listen(ServerOptions { host: "127.0.0.1".into(), tls: Some(config), ..Default::default() }).unwrap();
    let port = server.addr().port();

    // Untrusted by default: the test CA is not a public root.
    hub.connect(&format!("wss://localhost:{port}"), ClientOptions::default()).unwrap();
    match next(&hub, &accept) {
        Event::Error { error, .. } => assert!(error.contains("certificate"), "{error}"),
        e => panic!("unexpected {}", describe(&e)),
    }
    assert!(matches!(next(&hub, &accept), Event::Close { code: 1006, .. }));

    let trusted = tls::client(Some(&format!("{dir}ca.pem"))).unwrap();
    let opts = ClientOptions { tls: Some(trusted.clone()), ..Default::default() };
    let client = hub.connect(&format!("wss://localhost:{port}/secure"), opts).unwrap();
    let (mut peer, mut opened) = (None, false);
    while peer.is_none() || !opened {
        match next(&hub, &accept) {
            Event::Connection { conn, .. } => peer = Some(conn),
            Event::Open { id } => opened = id == client.id(),
            e => panic!("unexpected {}", describe(&e)),
        }
    }
    let peer = peer.unwrap();
    let data: Vec<u8> = (0..200_000u32).map(|i| i as u8).collect();
    client.send(&data, true).unwrap();
    peer.send(b"from server", false).unwrap();
    let mut got = 0;
    while got < 2 {
        match next(&hub, &accept) {
            Event::Message { id, data: d, .. } if id == peer.id() => {
                assert_eq!(d, data);
                got += 1;
            }
            Event::Message { id, data: d, .. } if id == client.id() => {
                assert_eq!(d, b"from server");
                got += 1;
            }
            e => panic!("unexpected {}", describe(&e)),
        }
    }
    peer.close(1000, "done");
    let mut codes = Vec::new();
    while codes.len() < 2 {
        match next(&hub, &accept) {
            Event::Close { code, reason, .. } => codes.push((code, reason)),
            e => panic!("unexpected {}", describe(&e)),
        }
    }
    codes.sort();
    assert_eq!(codes, [(1000, String::new()), (1000, "done".to_string())]);

    // A certificate for another name is rejected.
    let opts = ClientOptions { tls: Some(trusted), ..Default::default() };
    hub.connect(&format!("wss://127.0.0.1:{port}"), opts).unwrap();
    match next(&hub, &accept) {
        Event::Error { error, .. } => assert!(error.contains("certificate"), "{error}"),
        e => panic!("unexpected {}", describe(&e)),
    }
}

#[test]
fn close_notify_does_not_discard_websocket_close_reply() {
    use rustls::{ClientConnection, StreamOwned};
    use std::io::{Read, Write};
    use std::net::{Shutdown, TcpStream};
    use ws_core::proto::frame::{self, BINARY, CLOSE};

    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures/tls/");
    let hub = Hub::default();
    let config = tls::server(&format!("{dir}server.pem"), &format!("{dir}server-key.pem")).unwrap();
    let server =
        hub.listen(ServerOptions { host: "127.0.0.1".into(), tls: Some(config), ..Default::default() }).unwrap();
    let tcp = TcpStream::connect(server.addr()).unwrap();
    tcp.set_read_timeout(Some(WAIT)).unwrap();
    let config = tls::client(Some(&format!("{dir}ca.pem"))).unwrap();
    let conn = ClientConnection::new(config, "localhost".try_into().unwrap()).unwrap();
    let mut raw = StreamOwned::new(conn, tcp);
    raw.write_all(UPGRADE.as_bytes()).unwrap();
    let Event::Connection { conn, .. } = next(&hub, &accept) else { panic!("expected connection") };
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        let mut b = [0];
        raw.read_exact(&mut b).unwrap();
        head.push(b[0]);
    }
    let data = vec![7; 2 * 1024 * 1024];
    conn.send(&data, true).unwrap();
    raw.conn.writer().write_all(&frame::encode(CLOSE, &frame::close_payload(1000, ""), Some([1; 4]))).unwrap();
    raw.conn.send_close_notify();
    while raw.conn.wants_write() {
        raw.conn.write_tls(&mut raw.sock).unwrap();
    }
    raw.sock.shutdown(Shutdown::Write).unwrap();
    let mut bytes = Vec::new();
    raw.read_to_end(&mut bytes).unwrap();
    let mut expected = frame::encode(BINARY, &data, None);
    expected.extend(frame::encode(CLOSE, &frame::close_payload(1000, ""), None));
    assert_eq!(bytes.len(), expected.len());
    assert_eq!(bytes, expected);
}
