//! A strict, bounded HTTP/1.1 head parser and header helpers.
use std::collections::HashSet;

pub const MAX_HEAD: usize = 16 * 1024;
const MAX_HEADERS: usize = 64;
const RESERVED: &[&str] = &[
    "host",
    "upgrade",
    "connection",
    "content-length",
    "transfer-encoding",
    "sec-websocket-key",
    "sec-websocket-version",
    "sec-websocket-protocol",
    "sec-websocket-extensions",
    "sec-websocket-accept",
];

/// Lowercased names with their values, in order.
pub type Headers = Vec<(String, String)>;

/// Index just past the blank line ending the head, if complete.
pub fn end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
}

pub fn is_token(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}

fn is_value(s: &str) -> bool {
    s.bytes().all(|b| b == b'\t' || (b >= b' ' && b != 0x7f))
}

/// Checks a header the user wants to add to a client request.
pub fn check_header(name: &str, value: &str) -> Result<(), String> {
    if !is_token(name) || !is_value(value) {
        return Err(format!("invalid header: {name}"));
    }
    if RESERVED.contains(&name.to_ascii_lowercase().as_str()) {
        return Err(format!("header is managed by ws: {name}"));
    }
    Ok(())
}

/// Splits a complete head into its start line and headers.
pub(crate) fn parse(head: &[u8]) -> Option<(&str, Headers)> {
    if head.len() > MAX_HEAD {
        return None;
    }
    let text = std::str::from_utf8(head).ok()?.strip_suffix("\r\n\r\n")?;
    let mut lines = text.split("\r\n");
    let start = lines.next()?;
    let mut headers = Vec::new();
    for line in lines {
        let (name, value) = line.split_once(':')?;
        let value = value.trim_matches([' ', '\t']);
        if !is_token(name) || !is_value(value) || headers.len() == MAX_HEADERS {
            return None;
        }
        headers.push((name.to_ascii_lowercase(), value.to_string()));
    }
    is_value(start).then_some((start, headers))
}

pub(crate) fn all<'a>(headers: &'a Headers, name: &'a str) -> impl Iterator<Item = &'a str> {
    headers.iter().filter(move |(n, _)| n == name).map(|(_, v)| v.as_str())
}

/// The value of a header that must appear exactly once.
pub(crate) fn one<'a>(headers: &'a Headers, name: &'a str) -> Option<&'a str> {
    let mut it = all(headers, name);
    let v = it.next()?;
    it.next().is_none().then_some(v)
}

/// Whether a comma-separated header contains `token`, ignoring case.
pub(crate) fn has_token(headers: &Headers, name: &str, token: &str) -> bool {
    all(headers, name).flat_map(|v| v.split(',')).any(|t| t.trim_matches([' ', '\t']).eq_ignore_ascii_case(token))
}

/// A comma-separated list of distinct tokens across all such headers.
pub(crate) fn tokens(headers: &Headers, name: &str) -> Option<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    for item in all(headers, name).flat_map(|v| v.split(',')) {
        let item = item.trim_matches([' ', '\t']);
        if !is_token(item) || !seen.insert(item) {
            return None;
        }
        out.push(item.to_string());
    }
    Some(out)
}
