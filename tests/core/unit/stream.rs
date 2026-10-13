use super::*;
use crate::tls;
use rustls::{ClientConnection, ServerConnection};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures/tls/");

fn trickle(mut tcp: TcpStream, kind: u8) {
    // An incomplete TLS record never yields plaintext or a complete handshake.
    if tcp.write_all(&[kind, 3, 3, 3, 232]).is_err() {
        return;
    }
    for _ in 0..100 {
        if tcp.write_all(&[0]).is_err() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn tls_trickle_cannot_extend_deadline() {
    for established in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let peer = thread::spawn(move || {
            let mut tcp = TcpStream::connect(addr).unwrap();
            tcp.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            if established {
                let config = tls::client(Some(&format!("{DIR}ca.pem"))).unwrap();
                let mut client = ClientConnection::new(config, "localhost".try_into().unwrap()).unwrap();
                while client.is_handshaking() {
                    client.complete_io(&mut tcp).unwrap();
                }
            }
            trickle(tcp, if established { 23 } else { 22 });
        });
        let (tcp, _) = listener.accept().unwrap();
        let config = tls::server(&format!("{DIR}server.pem"), &format!("{DIR}server-key.pem")).unwrap();
        let tls = Some(ServerConnection::new(config).unwrap().into());
        let start = Instant::now();
        let mut stream = Stream::new(tcp, tls, start + Duration::from_millis(250), &|| true);
        let result = if established {
            stream.handshake().unwrap();
            match stream.read_head() {
                Err(HeadError::Io(e)) => Err(e),
                _ => panic!("expected timeout"),
            }
        } else {
            stream.handshake()
        };
        let elapsed = start.elapsed();
        drop(stream);
        peer.join().unwrap();
        assert!(elapsed < Duration::from_millis(750), "TLS trickle bypassed deadline: {elapsed:?}");
        assert_eq!(result.unwrap_err().kind(), ErrorKind::TimedOut);
    }
}
