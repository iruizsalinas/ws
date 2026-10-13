//! `ws://` and `wss://` URLs (RFC 6455 section 3).

use std::net::Ipv6Addr;

#[derive(Debug, Clone, PartialEq)]
pub struct Url {
    pub tls: bool,
    /// Host without IPv6 brackets, lowercased.
    pub host: String,
    pub port: u16,
    /// Request target: path plus query, always starting with `/`.
    pub target: String,
    /// Value for the `Host` header.
    pub authority: String,
}

pub fn parse(input: &str) -> Result<Url, &'static str> {
    let (scheme, rest) = input.split_once("://").ok_or("invalid URL")?;
    let tls = match scheme.to_ascii_lowercase().as_str() {
        "ws" => false,
        "wss" => true,
        _ => return Err("URL scheme must be ws or wss"),
    };
    if rest.bytes().any(|b| b <= b' ' || b >= 0x7f) {
        return Err("URL contains invalid characters");
    }
    if rest.contains('#') {
        return Err("URL must not contain a fragment");
    }
    let split = rest.find(['/', '?']).unwrap_or(rest.len());
    let (auth, target) = rest.split_at(split);
    if auth.contains('@') {
        return Err("URL must not contain credentials");
    }
    let (host, port) = if let Some(v6) = auth.strip_prefix('[') {
        let (addr, tail) = v6.split_once(']').ok_or("invalid IPv6 host")?;
        addr.parse::<Ipv6Addr>().map_err(|_| "invalid IPv6 host")?;
        let port = match tail {
            "" => None,
            t => Some(t.strip_prefix(':').ok_or("invalid URL port")?),
        };
        (addr, port)
    } else {
        match auth.split_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (auth, None),
        }
    };
    if host.is_empty() {
        return Err("URL host is empty");
    }
    if !auth.starts_with('[') && !host.bytes().all(|b| b.is_ascii_alphanumeric() || b"-._".contains(&b)) {
        return Err("invalid URL host");
    }
    let default = if tls { 443 } else { 80 };
    let port = match port {
        None => default,
        Some(p) if !p.is_empty() && p.len() <= 5 && p.bytes().all(|b| b.is_ascii_digit()) => {
            p.parse::<u16>().ok().filter(|&p| p != 0).ok_or("invalid URL port")?
        }
        Some(_) => return Err("invalid URL port"),
    };
    let host = host.to_ascii_lowercase();
    let bracketed = if host.contains(':') { format!("[{host}]") } else { host.clone() };
    let authority = if port == default { bracketed } else { format!("{bracketed}:{port}") };
    let target = match target.chars().next() {
        None => "/".to_string(),
        Some('?') => format!("/{target}"),
        _ => target.to_string(),
    };
    Ok(Url { tls, host, port, target, authority })
}
