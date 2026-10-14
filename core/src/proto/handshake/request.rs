//! Validate bounded HTTP upgrade requests before exposing them to the app.
use crate::proto::base64;
use crate::proto::http::{Headers, all, has_token, one, parse, tokens};
use std::net::Ipv6Addr;

/// A validated upgrade request.
#[derive(Debug)]
pub struct Request {
    pub target: String,
    pub headers: Headers,
    pub key: String,
    pub protocols: Vec<String>,
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

fn valid_target(target: &str) -> bool {
    target.starts_with('/') && !target.bytes().any(|b| b <= b' ' || b >= 0x7f || b == b'#')
}

/// Validates an upgrade request; `Err` is the HTTP status to reject with.
pub fn request(head: &[u8]) -> Result<Request, u16> {
    let (start, headers) = parse(head).ok_or(400u16)?;
    let mut parts = start.split(' ');
    let (Some(method), Some(target), Some(version), None) = (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(400);
    };
    if version != "HTTP/1.1" || !valid_target(target) {
        return Err(400);
    }
    if method != "GET" {
        return Err(405);
    }
    if !one(&headers, "host").is_some_and(valid_host) {
        return Err(400);
    }
    // Origin is a singleton (RFC 6454 section 7.3). Do not expose an
    // ambiguous first/last/joined value to the application's access check.
    if all(&headers, "origin").nth(1).is_some() {
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
    if key.len() != 24 || base64::decode(key).is_none_or(|k| k.len() != 16) {
        return Err(400);
    }
    let protocols = tokens(&headers, "sec-websocket-protocol").ok_or(400u16)?;
    Ok(Request { target: target.to_string(), key: key.to_string(), protocols, headers })
}
