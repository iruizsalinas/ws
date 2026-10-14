use super::*;

#[test]
fn partial_tls_records_count_as_inbound_activity() {
    use rustls::{ClientConnection, StreamOwned};
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;
    use ws_core::proto::frame;
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures/tls/");
    let hub = Hub::default();
    let config = tls::server(&format!("{dir}server.pem"), &format!("{dir}server-key.pem")).unwrap();
    let interval = Duration::from_millis(200);
    let server = hub
        .listen(ServerOptions {
            host: "127.0.0.1".into(),
            tls: Some(config),
            heartbeat: Some(interval),
            ..Default::default()
        })
        .unwrap();
    let tcp = TcpStream::connect(server.addr()).unwrap();
    tcp.set_read_timeout(Some(WAIT)).unwrap();
    let config = tls::client(Some(&format!("{dir}ca.pem"))).unwrap();
    let client = ClientConnection::new(config, "localhost".try_into().unwrap()).unwrap();
    let mut raw = StreamOwned::new(client, tcp);
    raw.write_all(UPGRADE.as_bytes()).unwrap();
    let Event::Connection { conn, .. } = next(&hub, &accept) else { panic!("expected connection") };
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        raw.read_exact(&mut byte).unwrap();
        head.push(byte[0]);
    }
    let data = vec![b'x'; 1024];
    raw.conn.writer().write_all(&frame::encode(frame::BINARY, &data, Some([1; 4]))).unwrap();
    let mut encrypted = Vec::new();
    while raw.conn.wants_write() {
        raw.conn.write_tls(&mut encrypted).unwrap();
    }
    // No TLS plaintext can be returned until the authenticated record is complete.
    // Keep delivering raw bytes for longer than an unanswered heartbeat would survive.
    for byte in &encrypted[..8] {
        raw.sock.write_all(&[*byte]).unwrap();
        std::thread::sleep(interval / 2);
        assert_eq!(conn.state(), State::Open, "active TLS transfer was mistaken for a silent peer");
    }
    raw.sock.write_all(&encrypted[8..]).unwrap();
    assert!(matches!(next(&hub, &accept), Event::Message { data: received, .. } if received == data));
    conn.terminate();
}
