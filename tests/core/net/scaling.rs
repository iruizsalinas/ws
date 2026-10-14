//! Concurrent connections and pending handshakes cannot monopolize the I/O pool.
use super::common::*;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use ws_core::*;

#[test]
fn many_connections_echo_and_release_every_handle() {
    const COUNT: usize = 128;
    let hub = Hub::default();
    let server = hub
        .listen(ServerOptions {
            host: "127.0.0.1".into(),
            heartbeat: None,
            max_connections: COUNT,
            ..Default::default()
        })
        .unwrap();
    let clients: Vec<_> = (0..COUNT)
        .map(|_| hub.connect(&url(&server), ClientOptions { heartbeat: None, ..Default::default() }).unwrap())
        .collect();
    let mut opened = HashSet::new();
    let mut peers = HashMap::new();
    while opened.len() < COUNT || peers.len() < COUNT {
        match next(&hub, &accept) {
            Event::Open { id } => {
                assert!(opened.insert(id));
            }
            Event::Connection { conn, .. } => {
                assert!(peers.insert(conn.id(), conn).is_none());
            }
            e => panic!("unexpected {}", describe(&e)),
        }
    }
    for client in &clients {
        client.send_text("shared runtime").unwrap();
    }
    let mut received = HashSet::new();
    while received.len() < COUNT {
        match next(&hub, &accept) {
            Event::Message { id, data, binary: false } => {
                assert_eq!(data, b"shared runtime");
                if let Some(peer) = peers.get(&id) {
                    peer.consumed(data.len());
                    peer.send(&data, false).unwrap();
                } else {
                    assert!(opened.contains(&id));
                    assert!(received.insert(id));
                }
            }
            e => panic!("unexpected {}", describe(&e)),
        }
    }
    for client in &clients {
        client.terminate();
    }
    let mut closed = HashSet::new();
    while closed.len() < COUNT * 2 {
        match next(&hub, &accept) {
            Event::Close { id, .. } => {
                assert!(closed.insert(id));
            }
            e => panic!("unexpected {}", describe(&e)),
        }
    }
    assert!(clients.iter().chain(peers.values()).all(|c| c.state() == State::Closed));
}

#[test]
fn silent_handshakes_do_not_delay_a_healthy_connection() {
    let hub = Hub::default();
    let server = listen(&hub);
    let silent: Vec<_> = (0..64).map(|_| Raw::connect(&server)).collect();
    let (client, peer) = pair(&hub, &server, ClientOptions::default());
    client.send_text("not blocked").unwrap();
    assert!(matches!(next(&hub, &accept), Event::Message { data, .. } if data == b"not blocked"));
    client.terminate();
    peer.terminate();
    server.close();
    for mut raw in silent {
        raw.0.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        raw.eof();
    }
}
