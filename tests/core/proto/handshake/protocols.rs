use super::*;

#[test]
fn large_protocol_lists_preserve_order_and_reject_duplicates_across_headers() {
    let protocols: Vec<_> = (0..2000).map(|i| format!("p{i}")).collect();
    let head = req(&format!("Sec-WebSocket-Protocol: {}\r\n", protocols.join(",")));
    assert_eq!(request(head.as_bytes()).unwrap().protocols, protocols);
    let duplicate =
        req(&format!("Sec-WebSocket-Protocol: {}\r\nSec-WebSocket-Protocol: p1999\r\n", protocols.join(",")));
    assert_eq!(status(&duplicate), 400);
    let distinct = request(req("Sec-WebSocket-Protocol: chat\r\nSec-WebSocket-Protocol: CHAT\r\n").as_bytes()).unwrap();
    assert_eq!(distinct.protocols, ["chat", "CHAT"], "protocol identifiers are case-sensitive");
}

#[test]
fn an_attacker_sized_key_is_rejected_as_an_invalid_nonce() {
    let head = req("").replace(KEY, &"A".repeat(12000));
    assert!(head.len() < ws_core::proto::http::MAX_HEAD);
    assert_eq!(status(&head), 400);
}
