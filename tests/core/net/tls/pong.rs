use super::*;

#[test]
fn deferred_pong_preserves_tls_record_order_after_a_large_backlog() {
    use rustls::{ClientConnection, StreamOwned};
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use ws_core::proto::frame::{self, BINARY, PING, PONG, TEXT};

    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures/tls/");
    let hub = Hub::default();
    let config = tls::server(&format!("{dir}server.pem"), &format!("{dir}server-key.pem")).unwrap();
    let server = hub
        .listen(ServerOptions { host: "127.0.0.1".into(), tls: Some(config), heartbeat: None, ..Default::default() })
        .unwrap();
    let tcp = TcpStream::connect(server.addr()).unwrap();
    tcp.set_read_timeout(Some(WAIT)).unwrap();
    tcp.set_write_timeout(Some(WAIT)).unwrap();
    let config = tls::client(Some(&format!("{dir}ca.pem"))).unwrap();
    let client = ClientConnection::new(config, "localhost".try_into().unwrap()).unwrap();
    let mut raw = StreamOwned::new(client, tcp);
    raw.write_all(UPGRADE.as_bytes()).unwrap();
    let Event::Connection { conn, .. } = next(&hub, &accept) else { panic!("expected connection") };
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        let mut b = [0];
        raw.read_exact(&mut b).unwrap();
        head.push(b[0]);
    }
    let big = vec![7; 32 << 20];
    conn.send(&big, true).unwrap();
    assert!(conn.buffered() > 1 << 20, "the test must exercise pong backpressure");
    raw.write_all(&frame::encode(PING, b"tls pong", Some([1; 4]))).unwrap();
    assert!(matches!(next(&hub, &accept), Event::Ping { data, .. } if data == b"tls pong"));
    assert_eq!(server_frame(&mut raw), (BINARY, big));
    assert_eq!(server_frame(&mut raw), (PONG, b"tls pong".to_vec()));
    conn.send_text("after pong").unwrap();
    assert_eq!(server_frame(&mut raw), (TEXT, b"after pong".to_vec()));
    conn.terminate();
}
