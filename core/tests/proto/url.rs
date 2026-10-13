use ws_core::proto::url::parse;

#[test]
fn parses() {
    let u = parse("wss://Example.com/chat?x=1").unwrap();
    assert_eq!((u.tls, u.host.as_str(), u.port, u.target.as_str()), (true, "example.com", 443, "/chat?x=1"));
    assert_eq!(u.authority, "example.com");
    let u = parse("ws://127.0.0.1:8080").unwrap();
    assert_eq!((u.port, u.target.as_str(), u.authority.as_str()), (8080, "/", "127.0.0.1:8080"));
    let u = parse("ws://[::1]:9000?a").unwrap();
    assert_eq!((u.host.as_str(), u.target.as_str(), u.authority.as_str()), ("::1", "/?a", "[::1]:9000"));
    assert_eq!(parse("ws://[::1]").unwrap().authority, "[::1]");
}

#[test]
fn rejects() {
    for bad in [
        "http://a",
        "ws:/a",
        "ws://",
        "ws://a:0",
        "ws://a:70000",
        "ws://a:",
        "ws://a#x",
        "ws://u@a",
        "ws://a b",
        "ws://[::1",
        "ws://[zz]",
        "ws://[::1]x",
        "ws://a/\u{e9}",
        "ws://a\r\n",
    ] {
        assert!(parse(bad).is_err(), "{bad}");
    }
}
