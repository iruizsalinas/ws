use super::*;

#[test]
fn client_heartbeat_pings_and_hides_its_pongs() {
    let hub = Hub::default();
    let server = server(&hub, None);
    let (client, peer) = pair(&hub, &server, ClientOptions { heartbeat: Some(IV), ..Default::default() });
    let mut pings = 0;
    while pings < 2 {
        match next(&hub, &accept) {
            // The server's application sees the client's pings...
            Event::Ping { id, data } => {
                assert_eq!((id, data.len()), (peer.id(), 8));
                pings += 1;
            }
            // ...but the client's never sees the automatic pongs.
            e => panic!("unexpected {}", describe(&e)),
        }
    }
    assert_eq!(client.state(), State::Open);
    client.terminate();
    peer.terminate();
}

#[test]
fn tls_heartbeat_responses_keep_both_endpoints_open() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures/tls/");
    let hub = Hub::default();
    let server = hub
        .listen(ServerOptions {
            host: "127.0.0.1".into(),
            tls: Some(tls::server(&format!("{dir}server.pem"), &format!("{dir}server-key.pem")).unwrap()),
            heartbeat: Some(IV),
            ..Default::default()
        })
        .unwrap();
    let client = hub
        .connect(
            &format!("wss://localhost:{}", server.addr().port()),
            ClientOptions {
                tls: Some(tls::client(Some(&format!("{dir}ca.pem"))).unwrap()),
                heartbeat: Some(IV),
                ..Default::default()
            },
        )
        .unwrap();
    let (mut peer, mut opened) = (None, false);
    while peer.is_none() || !opened {
        match next(&hub, &accept) {
            Event::Connection { conn, .. } => peer = Some(conn),
            Event::Open { id } => opened = id == client.id(),
            e => panic!("unexpected {}", describe(&e)),
        }
    }
    let peer = peer.unwrap();
    // Repeated rounds exercise marker transmission, deferred pong encryption,
    // and fast replies. Automatic heartbeat pongs remain internal.
    for _ in 0..3 {
        assert!(matches!(next(&hub, &accept), Event::Ping { data, .. } if data.len() == 8));
    }
    assert_eq!(client.state(), State::Open);
    assert_eq!(peer.state(), State::Open);
    client.terminate();
    peer.terminate();
}
