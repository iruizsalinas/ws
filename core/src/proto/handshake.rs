//! The opening handshake (RFC 6455 section 4).

use super::http::{Headers, all, has_token, one, parse, tokens};
use super::{base64, url::Url};
use ring::digest::{SHA1_FOR_LEGACY_USE_ONLY, digest};
use std::net::Ipv6Addr;

const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// A validated upgrade request.
#[derive(Debug)]
pub struct Request {
    pub target: String,
    pub headers: Headers,
    pub key: String,
    pub protocols: Vec<String>,
}

pub fn accept_key(key: &str) -> String {
    base64::encode(digest(&SHA1_FOR_LEGACY_USE_ONLY, format!("{key}{GUID}").as_bytes()).as_ref())
}

fn valid_host(host: &str) -> bool {
    let (name_ok, port) = match host.strip_prefix('[') {
        Some(v6) => match v6.split_once(']') {
            Some((addr, tail)) => (addr.parse::<Ipv6Addr>().is_ok(), tail),
            None => return false,
        },
        None => {
            let (name, tail) = host.split_at(host.find(':').unwrap_or(host.len()));
            let chars = |b: u8| b.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=%".contains(&b);
            (!name.is_empty() && name.bytes().all(chars), tail)
        }
    };
    let digits = |p: &str| !p.is_empty() && p.len() <= 5 && p.bytes().all(|b| b.is_ascii_digit());
    name_ok && (port.is_empty() || port.strip_prefix(':').is_some_and(digits))
}

/// Validates an upgrade request; `Err` is the HTTP status to reject with.
pub fn request(head: &[u8]) -> Result<Request, u16> {
    let (start, headers) = parse(head).ok_or(400u16)?;
    let parts: Vec<&str> = start.split(' ').collect();
    let [method, target, version] = parts[..] else {
        return Err(400);
    };
    if version != "HTTP/1.1" || !target.starts_with('/') {
        return Err(400);
    }
    if method != "GET" {
        return Err(405);
    }
    if !one(&headers, "host").is_some_and(valid_host) {
        return Err(400);
    }
    // A body would be read as frames: refuse anything that declares one.
    if all(&headers, "transfer-encoding").next().is_some() || all(&headers, "content-length").any(|v| v != "0") {
        return Err(400);
    }
    if !has_token(&headers, "upgrade", "websocket") {
        return Err(426);
    }
    if !has_token(&headers, "connection", "upgrade") {
        return Err(400);
    }
    if one(&headers, "sec-websocket-version") != Some("13") {
        return Err(426);
    }
    let key = one(&headers, "sec-websocket-key").ok_or(400u16)?;
    if base64::decode(key).is_none_or(|k| k.len() != 16) {
        return Err(400);
    }
    let protocols = tokens(&headers, "sec-websocket-protocol").ok_or(400u16)?;
    Ok(Request { target: target.to_string(), key: key.to_string(), protocols, headers })
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
    let status = match (parts.next(), parts.next()) {
        (Some("HTTP/1.1"), Some(s)) if s.len() == 3 && s.bytes().all(|b| b.is_ascii_digit()) => s,
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
