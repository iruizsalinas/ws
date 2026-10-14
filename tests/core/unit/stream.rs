use super::*;
use crate::{
    net::{runtime, transport},
    tls,
};
use rustls::ClientConnection;
use std::io::Write;
use std::net::TcpListener;
use std::thread;
use std::time::{Duration, Instant};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures/tls/");

fn trickle(mut tcp: TcpStream, kind: u8) {
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
        tcp.set_nonblocking(true).unwrap();
        let config = tls::server(&format!("{DIR}server.pem"), &format!("{DIR}server-key.pem")).unwrap();
        let start = Instant::now();
        let result = runtime::shared().unwrap().block_on(async {
            let tcp = tokio::net::TcpStream::from_std(tcp).unwrap();
            runtime::until(start + Duration::from_millis(250), async {
                let mut stream = transport::server(tcp, Some(config)).await?;
                if established {
                    match stream.read_head().await {
                        Err(HeadError::Io(e)) => Err(e),
                        _ => panic!("expected timeout"),
                    }
                } else {
                    Ok(())
                }
            })
            .await
        });
        let elapsed = start.elapsed();
        peer.join().unwrap();
        assert!(elapsed < Duration::from_millis(750), "TLS trickle bypassed deadline: {elapsed:?}");
        assert_eq!(result.unwrap_err().kind(), ErrorKind::TimedOut);
    }
}
