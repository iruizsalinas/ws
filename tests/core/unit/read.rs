use super::*;
use crate::net::link::Config;
use crate::net::link::handle::INBOX_LIMIT;
use crate::tls;
use rustls::{ClientConnection, ServerConnection};
use std::io::Write;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::mpsc::channel;
use std::time::Duration;

fn pair() -> (Connection, Connection) {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures/tls/");
    let config = tls::server(&format!("{dir}server.pem"), &format!("{dir}server-key.pem")).unwrap();
    let mut server = Connection::from(ServerConnection::new(config).unwrap());
    let config = tls::client(Some(&format!("{dir}ca.pem"))).unwrap();
    let mut client = Connection::from(ClientConnection::new(config, "localhost".try_into().unwrap()).unwrap());
    for _ in 0..10 {
        for outgoing in [true, false] {
            let (from, to) = if outgoing { (&mut client, &mut server) } else { (&mut server, &mut client) };
            let mut bytes = Vec::new();
            while from.wants_write() {
                from.write_tls(&mut bytes).unwrap();
            }
            if !bytes.is_empty() {
                to.read_tls(&mut bytes.as_slice()).unwrap();
                to.process_new_packets().unwrap();
            }
        }
        if !client.is_handshaking() && !server.is_handshaking() {
            break;
        }
    }
    assert!(!server.is_handshaking());
    (client, server)
}

#[test]
fn bytes_after_tls_close_do_not_spin() {
    let (mut client, mut server) = pair();
    client.send_close_notify();
    let mut close = Vec::new();
    client.write_tls(&mut close).unwrap();
    server.read_tls(&mut close.as_slice()).unwrap();
    assert!(server.process_new_packets().unwrap().peer_has_closed());
    let (events, _) = channel();
    let link = Link::new(1, true, Config::test(1024), events.into());
    let (done, result) = channel();
    let worker = std::thread::spawn(move || {
        let outcome = link.open(&Mutex::new(server), &[0; 16], &mut Vec::new());
        done.send(outcome).unwrap();
    });
    assert_eq!(result.recv_timeout(Duration::from_secs(1)).expect("TLS reader spun after close_notify"), Ok(false));
    worker.join().unwrap();
}

#[test]
fn tls_plaintext_is_drained_between_records() {
    for chunk in [4096, 8191, 16383, 16384] {
        let (mut client, server) = pair();
        let expected = vec![42; 48 * 1024];
        client.writer().write_all(&expected).unwrap();
        let mut encrypted = Vec::new();
        while client.wants_write() {
            client.write_tls(&mut encrypted).unwrap();
        }
        let (events, _) = channel();
        let link = Link::new(1, true, Config::test(expected.len()), events.into());
        let server = Mutex::new(server);
        let mut plain = Vec::new();
        for bytes in encrypted.chunks(chunk) {
            assert_eq!(link.open(&server, bytes, &mut plain), Ok(true), "chunk size {chunk}");
        }
        assert_eq!(plain, expected);
    }
}

#[test]
fn closing_connections_do_not_wait_for_application_consumption() {
    let (events, _rx) = channel();
    let link = Link::new(1, true, Config::test(1024), events.into());
    link.inbox.store(INBOX_LIMIT, Relaxed);
    lock(&link.flags).state = State::Closing;
    assert!(link.wait_inbox(), "backpressure must not prevent reading the peer's close reply");
}

#[test]
fn cancelled_start_closes_the_transport_even_when_the_handle_is_retained() {
    use std::io::Read;
    use std::net::{TcpListener, TcpStream};

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    peer.set_read_timeout(Some(Duration::from_millis(200))).unwrap();
    let (tcp, _) = listener.accept().unwrap();
    let (events, _rx) = channel();
    let link = Link::new(1, false, Config::test(1024), events.into());
    link.terminate();
    link.start(tcp, None, vec![], None, Event::Open { id: 1 });
    assert_eq!(peer.read(&mut [0]).expect("cancelled connection retained a live socket"), 0);
    assert_eq!(link.state(), State::Closed);
}
