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
