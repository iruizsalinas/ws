use super::*;
use rustls::{ClientConnection, StreamOwned};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;
use ws_core::proto::frame;

#[test]
fn idle_tls_key_update_is_answered_without_a_websocket_write_or_heartbeat() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures/tls/");
    let hub = Hub::default();
    let server = hub
        .listen(ServerOptions {
            host: "127.0.0.1".into(),
            tls: Some(tls::server(&format!("{dir}server.pem"), &format!("{dir}server-key.pem")).unwrap()),
            heartbeat: None,
            ..Default::default()
        })
        .unwrap();
    let tcp = TcpStream::connect(server.addr()).unwrap();
    tcp.set_read_timeout(Some(WAIT)).unwrap();
    let client =
        ClientConnection::new(tls::client(Some(&format!("{dir}ca.pem"))).unwrap(), "localhost".try_into().unwrap())
            .unwrap();
    let mut raw = StreamOwned::new(client, tcp);
    raw.write_all(UPGRADE.as_bytes()).unwrap();
    let Event::Connection { conn, .. } = next(&hub, &accept) else { panic!("expected connection") };
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        raw.read_exact(&mut byte).unwrap();
        head.push(byte[0]);
    }
    // Consume any initial session tickets before looking for the update reply.
    raw.sock.set_read_timeout(Some(Duration::from_millis(100))).unwrap();
    while raw.conn.read_tls(&mut raw.sock).is_ok_and(|n| n > 0) {
        raw.conn.process_new_packets().unwrap();
    }
    raw.conn.refresh_traffic_keys().unwrap();
    while raw.conn.wants_write() {
        raw.conn.write_tls(&mut raw.sock).unwrap();
    }
    raw.sock.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    assert!(raw.conn.read_tls(&mut raw.sock).expect("idle KeyUpdate reply was never flushed") > 0);
    raw.conn.process_new_packets().unwrap();
    // New traffic must still decrypt correctly after both directions rotate.
    raw.write_all(&frame::encode(frame::BINARY, b"after update", Some([1; 4]))).unwrap();
    assert!(matches!(next(&hub, &accept), Event::Message { data, .. } if data == b"after update"));
    conn.terminate();
}
