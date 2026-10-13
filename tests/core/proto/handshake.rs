use ws_core::proto::handshake::{accept_key, reject, request, response};
use ws_core::proto::http::check_header;

const KEY: &str = "dGhlIHNhbXBsZSBub25jZQ==";

fn req(extra: &str) -> String {
    format!(
        "GET /chat HTTP/1.1\r\nHost: example.com\r\nUpgrade: websocket\r\nConnection: keep-alive, Upgrade\r\nSec-WebSocket-Key: {KEY}\r\nSec-WebSocket-Version: 13\r\n{extra}\r\n"
    )
}

fn status(head: &str) -> u16 {
    request(head.as_bytes()).unwrap_err()
}

#[test]
fn rfc_accept_key() {
    assert_eq!(accept_key(KEY), "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
}

#[test]
fn valid_request() {
    let r =
        request(req("Sec-WebSocket-Protocol: chat, superchat\r\nOrigin: http://example.com\r\n").as_bytes()).unwrap();
    assert_eq!(r.target, "/chat");
    assert_eq!(r.protocols, ["chat", "superchat"]);
    assert!(r.headers.iter().any(|(n, v)| n == "origin" && v == "http://example.com"));
}

#[test]
fn rejected_requests() {
    let cases = [
        (req("").replacen("GET /chat", "POST /chat", 1), 405),
        (req("").replacen("HTTP/1.1\r\n", "HTTP/1.0\r\n", 1), 400),
        (req("").replacen("GET /chat", "GET http://x/chat", 1), 400),
        (req("").replacen("Upgrade: websocket\r\n", "", 1), 426),
        (req("").replacen("Version: 13", "Version: 8", 1), 426),
        (req("").replacen("keep-alive, Upgrade", "keep-alive", 1), 400),
        (req("").replacen(KEY, "c2hvcnQ=", 1), 400),
        (req("Sec-WebSocket-Key: AAAAAAAAAAAAAAAAAAAAAA==\r\n"), 400),
        (req("Origin: https://trusted.example\r\nOrigin: https://other.example\r\n"), 400),
        (req("Content-Length: 5\r\n"), 400),
        (req("Transfer-Encoding: chunked\r\n"), 400),
        (req("Sec-WebSocket-Protocol: a b\r\n"), 400),
        (req("Sec-WebSocket-Protocol: a, a\r\n"), 400),
        (req("Sec-WebSocket-Protocol: \u{a0}chat\r\n"), 400),
        (req("").replace("keep-alive, Upgrade", "\u{a0}Upgrade"), 400),
        (req(" folded: x\r\n"), 400),
        (req("X: a\nb\r\n"), 400),
        (req("").replace("/chat", "/chat\tother"), 400),
        (req("").replace("/chat", "/chat#fragment"), 400),
        (req("").replace("/chat", "/caf\u{e9}"), 400),
        ("GET / HTTP/1.1\n\n".to_string(), 400),
    ];
    for (head, expected) in cases {
        assert_eq!(status(&head), expected, "{head}");
    }
}

#[test]
fn hosts() {
    let with_host = |h: &str| req("").replacen("Host: example.com", &format!("Host: {h}"), 1);
    for ok in ["a", "a:1", "127.0.0.1:8080", "[::1]", "[::1]:80", "xn--d1a.example"] {
        assert!(request(with_host(ok).as_bytes()).is_ok(), "{ok}");
    }
    for bad in ["", ":80", "a:", "a:123456", "[::1", "[zz]", "a b", "a/b", "[::1]x"] {
        assert_eq!(status(&with_host(bad)), 400, "{bad}");
    }
    assert_eq!(status(&req("").replacen("Host: example.com\r\n", "", 1)), 400);
    assert_eq!(status(&req("Host: other.com\r\n")), 400);
}

#[test]
fn header_limit() {
    let many: String = (0..64).map(|i| format!("X-{i}: v\r\n")).collect();
    assert_eq!(status(&req(&many)), 400);
}

#[test]
fn responses() {
    let ok = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\nSec-WebSocket-Protocol: chat\r\n\r\n",
        accept_key(KEY)
    );
    assert_eq!(response(ok.as_bytes(), KEY, &["chat".into()]).unwrap().as_deref(), Some("chat"));
    assert!(response(ok.as_bytes(), KEY, &[]).is_err());
    assert!(response(ok.as_bytes(), "other", &["chat".into()]).is_err());
    let err = response(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n", KEY, &[]).unwrap_err();
    assert_eq!(err, "unexpected server response: 403");
    let ext = ok.replace("\r\n\r\n", "\r\nSec-WebSocket-Extensions: permessage-deflate\r\n\r\n");
    assert!(response(ext.as_bytes(), KEY, &["chat".into()]).is_err());
}

#[test]
fn rejects_have_no_body() {
    let r = String::from_utf8(reject(426)).unwrap();
    assert!(r.starts_with("HTTP/1.1 426 Upgrade Required\r\n") && r.ends_with("Sec-WebSocket-Version: 13\r\n\r\n"));
    assert!(r.contains("Content-Length: 0\r\n"));
}

#[test]
fn user_headers() {
    assert!(check_header("Origin", "https://a").is_ok());
    assert!(check_header("Host", "x").is_err());
    assert!(check_header("X", "a\r\nb").is_err());
    assert!(check_header("Bad Name", "x").is_err());
}

#[test]
fn switching_response_status_line() {
    let valid = String::from_utf8(ws_core::proto::handshake::switching(KEY, None)).unwrap();
    // The reason phrase may be empty, but its preceding space is required (RFC 9112 section 4).
    assert!(response(valid.replace("101 Switching Protocols", "101").as_bytes(), KEY, &[]).is_err());
    assert!(response(valid.replace("Switching Protocols", "").as_bytes(), KEY, &[]).is_ok());
    // A 1xx response has no body, so framing headers are ignored (RFC 9112 section 6.3).
    for framing in ["Content-Length: 0", "Transfer-Encoding: chunked"] {
        let head = valid.replace("\r\n\r\n", &format!("\r\n{framing}\r\n\r\n"));
        assert!(response(head.as_bytes(), KEY, &[]).is_ok(), "rejected {framing}");
    }
}

#[test]
fn direct_handshake_parsing_enforces_byte_limit() {
    let huge = req(&format!("X-Large: {}\r\n", "x".repeat(16 * 1024)));
    assert!(request(huge.as_bytes()).is_err());
}
