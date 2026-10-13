//! Connecting, aborting, timeouts and shutting down.

use super::common::*;
use std::net::TcpStream;
use std::time::{Duration, Instant};
use ws_core::*;

#[test]
fn terminate_and_abort() {
    let hub = Hub::default();
    let server = listen(&hub);
    let (client, peer) = pair(&hub, &server, ClientOptions::default());
    client.terminate();
    let mut closed = Vec::new();
    while closed.len() < 2 {
        match next(&hub, &accept) {
            Event::Close { id, code, .. } => closed.push((id, code)),
            e => panic!("unexpected {}", describe(&e)),
        }
    }
    closed.sort();
    assert_eq!(closed, [(client.id(), 1006), (peer.id(), 1006)]);

    // Closing while connecting aborts with an error, like browsers.
    let blackhole = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let c = hub
        .connect(&format!("ws://127.0.0.1:{}", blackhole.local_addr().unwrap().port()), ClientOptions::default())
        .unwrap();
    c.close(1000, "").unwrap();
    assert_eq!(describe(&next(&hub, &accept)), format!("error {} connection closed before it was established", c.id()));
    assert_eq!(describe(&next(&hub, &accept)), format!("close {} 1006 ", c.id()));
}

#[test]
fn handshake_timeout() {
    let hub = Hub::default();
    let silent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let opts = ClientOptions { timeout: Duration::from_millis(300), ..Default::default() };
    let start = Instant::now();
    let c = hub.connect(&format!("ws://127.0.0.1:{}", silent.local_addr().unwrap().port()), opts).unwrap();
    match next(&hub, &accept) {
        Event::Error { error, .. } => assert!(error.contains("timed out"), "{error}"),
        e => panic!("unexpected {}", describe(&e)),
    }
    assert!(start.elapsed() < Duration::from_secs(3));
    assert_eq!(describe(&next(&hub, &accept)), format!("close {} 1006 ", c.id()));
}

#[test]
fn connection_refused() {
    let hub = Hub::default();
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    hub.connect(&format!("ws://127.0.0.1:{port}"), ClientOptions::default()).unwrap();
    match next(&hub, &accept) {
        Event::Error { error, .. } => assert!(error.starts_with("cannot connect"), "{error}"),
        e => panic!("unexpected {}", describe(&e)),
    }
}

#[test]
fn invalid_arguments() {
    let hub = Hub::default();
    assert!(hub.connect("http://x", ClientOptions::default()).is_err());
    let bad = ClientOptions { protocols: vec!["a b".into()], ..Default::default() };
    assert!(hub.connect("ws://x", bad).is_err());
    let bad = ClientOptions { headers: vec![("Sec-WebSocket-Key".into(), "x".into())], ..Default::default() };
    assert!(hub.connect("ws://x", bad).is_err());
    let bad = ClientOptions { timeout: Duration::MAX, ..Default::default() };
    assert!(hub.connect("ws://127.0.0.1:1", bad).is_err(), "overflowing timeout must not panic in the worker");
}

#[test]
fn server_close_stops_accepting() {
    let hub = Hub::default();
    let server = listen(&hub);
    let addr = server.addr();
    server.close();
    let deadline = Instant::now() + WAIT;
    while TcpStream::connect(addr).is_ok() {
        assert!(Instant::now() < deadline, "listener still accepting");
        std::thread::sleep(Duration::from_millis(20));
    }
}
