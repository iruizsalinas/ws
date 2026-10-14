//! The opening handshake (RFC 6455 section 4).

use super::http::{Headers, all, has_token, one, parse};
use super::{base64, url::Url};
use ring::digest::{SHA1_FOR_LEGACY_USE_ONLY, digest};
mod request;
pub use request::{Request, request};

const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

pub fn accept_key(key: &str) -> String {
    base64::encode(digest(&SHA1_FOR_LEGACY_USE_ONLY, format!("{key}{GUID}").as_bytes()).as_ref())
}

pub fn switching(key: &str, protocol: Option<&str>) -> Vec<u8> {
    let protocol = protocol.map(|p| format!("Sec-WebSocket-Protocol: {p}\r\n")).unwrap_or_default();
    let accept = accept_key(key);
    format!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n{protocol}\r\n")
        .into_bytes()
}

fn reason(status: u16) -> &'static str {
    match status {
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        426 => "Upgrade Required",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Error",
    }
}

/// A rejection with no body: the status code says it all.
pub fn reject(status: u16) -> Vec<u8> {
    let extra = match status {
        405 => "Allow: GET\r\n",
        426 => "Upgrade: websocket\r\nSec-WebSocket-Version: 13\r\n",
        _ => "",
    };
    format!("HTTP/1.1 {status} {}\r\nConnection: close\r\nContent-Length: 0\r\n{extra}\r\n", reason(status))
        .into_bytes()
}

pub fn client_request(url: &Url, key: &str, protocols: &[String], headers: &Headers) -> Vec<u8> {
    let mut out = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n",
        url.target, url.authority
    );
    if !protocols.is_empty() {
        out += &format!("Sec-WebSocket-Protocol: {}\r\n", protocols.join(", "));
    }
    for (name, value) in headers {
        out += &format!("{name}: {value}\r\n");
    }
    (out + "\r\n").into_bytes()
}

/// Validates the server's reply; returns the selected subprotocol.
pub fn response(head: &[u8], key: &str, protocols: &[String]) -> Result<Option<String>, String> {
    let (start, headers) = parse(head).ok_or("invalid handshake response")?;
    let mut parts = start.splitn(3, ' ');
    let status = match (parts.next(), parts.next(), parts.next()) {
        (Some("HTTP/1.1"), Some(s), Some(_)) if s.len() == 3 && s.bytes().all(|b| b.is_ascii_digit()) => s,
        _ => return Err("invalid handshake response".into()),
    };
    if status != "101" {
        return Err(format!("unexpected server response: {status}"));
    }
    if !has_token(&headers, "upgrade", "websocket") || !has_token(&headers, "connection", "upgrade") {
        return Err("invalid upgrade headers".into());
    }
    if one(&headers, "sec-websocket-accept") != Some(accept_key(key).as_str()) {
        return Err("invalid Sec-WebSocket-Accept".into());
    }
    if all(&headers, "sec-websocket-extensions").next().is_some() {
        return Err("server sent an extension that was not offered".into());
    }
    let mut chosen = all(&headers, "sec-websocket-protocol");
    match (chosen.next(), chosen.next()) {
        (None, _) => Ok(None),
        (Some(p), None) if protocols.iter().any(|o| o == p) => Ok(Some(p.to_string())),
        _ => Err("server sent a subprotocol that was not offered".into()),
    }
}
