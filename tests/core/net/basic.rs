//! Messaging, ping/pong, subprotocols and limits between real endpoints.

use super::common::*;
use ws_core::*;

#[test]
fn rust_api_rejects_invalid_outbound_frames() {
    let hub = Hub::default();
    let server = listen(&hub);
    let (client, peer) = pair(&hub, &server, ClientOptions::default());
    assert!(client.send(&[0xff], false).is_err());
    client.send_text("valid \u{e9} text").unwrap();
    assert!(
        matches!(next(&hub, &accept), Event::Message { binary: false, data, .. } if data == "valid \u{e9} text".as_bytes())
    );
    assert!(client.ping(&[0; 126]).is_err());
    for (code, reason) in [(1005, String::new()), (1000, "x".repeat(124))] {
        assert!(client.close(code, &reason).is_err());
        assert_eq!(client.state(), State::Open, "invalid close must not change state");
    }
    client.send(&[0xff], true).unwrap();
    assert!(matches!(next(&hub, &accept), Event::Message { binary: true, data, .. } if data == [0xff]));
    client.terminate();
    peer.terminate();
}

#[test]
fn echo_and_close() {
    let hub = Hub::default();
    let server = listen(&hub);
    let (client, peer) = pair(&hub, &server, ClientOptions::default());
    assert_eq!(client.state(), State::Open);
    client.send("h\u{e9}llo".as_bytes(), false).unwrap();
    client.send(&[0, 1, 2, 255], true).unwrap();
    let big = vec![7u8; 300_000];
    client.send(&big, true).unwrap();
    let mut got = Vec::new();
    while got.len() < 3 {
        match next(&hub, &accept) {
            Event::Message { id, data, binary } if id == peer.id() => got.push((data, binary)),
            e => panic!("unexpected {}", describe(&e)),
        }
    }
    assert_eq!(got, [("h\u{e9}llo".as_bytes().to_vec(), false), (vec![0, 1, 2, 255], true), (big, true)]);

    client.close(4000, "bye").unwrap();
    assert_eq!(client.send(b"late", false), Err("closed"));
    let mut closed = Vec::new();
    while closed.len() < 2 {
        match next(&hub, &accept) {
            Event::Close { id, code, reason } => closed.push((id, code, reason)),
            e => panic!("unexpected {}", describe(&e)),
        }
    }
    closed.sort();
    // The initiator sees the echoed code; the reason travels one way.
    assert_eq!(closed, [(client.id(), 4000, String::new()), (peer.id(), 4000, "bye".to_string())]);
    assert_eq!(client.state(), State::Closed);
    assert_eq!(peer.state(), State::Closed);
}

#[test]
fn ping_pong() {
    let hub = Hub::default();
    let server = listen(&hub);
    let (client, peer) = pair(&hub, &server, ClientOptions::default());
    client.ping(b"hi").unwrap();
    let mut seen = Vec::new();
    while seen.len() < 2 {
        match next(&hub, &accept) {
            Event::Ping { id, data } => seen.push(("ping", id, data)),
            Event::Pong { id, data } => seen.push(("pong", id, data)),
            e => panic!("unexpected {}", describe(&e)),
        }
    }
    assert_eq!(seen, [("ping", peer.id(), b"hi".to_vec()), ("pong", client.id(), b"hi".to_vec())]);
}

#[test]
fn subprotocol_selection() {
    let hub = Hub::default();
    let server = listen(&hub);
    let opts = ClientOptions { protocols: vec!["a".into(), "chat".into()], ..Default::default() };
    let client = hub.connect(&url(&server), opts).unwrap();
    let pick = |r: &Request| {
        assert_eq!(r.protocols, ["a", "chat"]);
        Reply::Accept(Some("chat".into()))
    };
    let mut done = 0;
    while done < 2 {
        match next(&hub, &pick) {
            Event::Connection { conn, .. } => {
                assert_eq!(conn.protocol(), Some("chat"));
                done += 1;
            }
            Event::Open { .. } => {
                assert_eq!(client.protocol(), Some("chat"));
                done += 1;
            }
            e => panic!("unexpected {}", describe(&e)),
        }
    }
}

#[test]
fn rejected_upgrade() {
    let hub = Hub::default();
    let server = listen(&hub);
    let client = hub.connect(&url(&server), ClientOptions::default()).unwrap();
    let deny = |_: &Request| Reply::Reject(403);
    match next(&hub, &deny) {
        Event::Error { id, error } => {
            assert_eq!((id, error.as_str()), (client.id(), "unexpected server response: 403"))
        }
        e => panic!("unexpected {}", describe(&e)),
    }
    match next(&hub, &deny) {
        Event::Close { code, .. } => assert_eq!(code, 1006),
        e => panic!("unexpected {}", describe(&e)),
    }
}

#[test]
fn message_too_big() {
    let hub = Hub::default();
    let server =
        hub.listen(ServerOptions { host: "127.0.0.1".into(), max_message: 1000, ..Default::default() }).unwrap();
    let (client, peer) = pair(&hub, &server, ClientOptions::default());
    client.send(&[1; 1001], true).unwrap();
    let mut events = Vec::new();
    while events.len() < 3 {
        events.push(describe(&next(&hub, &accept)));
    }
    events.sort();
    let (c, p) = (client.id(), peer.id());
    let mut expected = [format!("close {c} 1009 "), format!("close {p} 1009 "), format!("error {p} message too big")];
    expected.sort();
    assert_eq!(events, expected);
}
