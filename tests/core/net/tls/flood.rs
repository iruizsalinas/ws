use super::*;
use rustls::ClientConnection;
use std::io::Write;
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

#[test]
fn established_connections_progress_during_repeated_tls_client_hello_floods() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures/tls/");
    let hub = Hub::default();
    let server = listen(&hub);
    let (client, peer) = pair(&hub, &server, ClientOptions { heartbeat: None, ..Default::default() });
    let tls_server = hub
        .listen(ServerOptions {
            host: "127.0.0.1".into(),
            tls: Some(tls::server(&format!("{dir}server.pem"), &format!("{dir}server-key.pem")).unwrap()),
            heartbeat: None,
            ..Default::default()
        })
        .unwrap();
    let mut handshake = ClientConnection::new(tls::client(None).unwrap(), "localhost".try_into().unwrap()).unwrap();
    let mut hello = Vec::new();
    handshake.write_tls(&mut hello).unwrap();
    let addr = tls_server.addr();
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();
    let (started, ready) = std::sync::mpsc::channel();
    let flood = std::thread::spawn(move || {
        let mut sockets = Vec::new();
        let mut sent = 0;
        while !worker_stop.load(Relaxed) {
            // A saturated listener may drop or defer new admissions; that's
            // expected. The assertion concerns established connections.
            let Ok(mut tcp) = TcpStream::connect_timeout(&addr, std::time::Duration::from_secs(1)) else { continue };
            tcp.set_write_timeout(Some(std::time::Duration::from_secs(1))).unwrap();
            let _ = tcp.write_all(&hello);
            sockets.push(tcp);
            sent += 1;
            if sockets.len() == 128 {
                if sent == 128 {
                    started.send(()).unwrap();
                }
                sockets.clear();
            }
        }
        sent
    });
    // Stop the traffic even if an assertion fails, so teardown can't leave a worker.
    struct Stop(Arc<AtomicBool>);
    impl Drop for Stop {
        fn drop(&mut self) {
            self.0.store(true, Relaxed);
        }
    }
    let guard = Stop(stop.clone());
    ready.recv_timeout(WAIT).unwrap();
    for _ in 0..200 {
        client.send_text("during TLS load").unwrap();
        assert!(matches!(next(&hub, &accept), Event::Message { id, data, .. }
            if id == peer.id() && data == b"during TLS load"));
        peer.consumed(b"during TLS load".len());
        peer.send_text("reply").unwrap();
        assert!(matches!(next(&hub, &accept), Event::Message { id, data, .. }
            if id == client.id() && data == b"reply"));
        client.consumed(5);
    }
    drop(guard);
    assert!(flood.join().unwrap() >= 128);
    client.terminate();
    peer.terminate();
    tls_server.close();
}
