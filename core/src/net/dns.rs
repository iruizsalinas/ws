//! Bound OS resolver work, including resolutions whose callers were cancelled.
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::sync::{Arc, OnceLock};
use tokio::sync::Semaphore;

pub(super) const LIMIT: usize = 16;

pub(super) async fn resolve(host: &str, port: u16) -> io::Result<Vec<SocketAddr>> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(vec![SocketAddr::new(ip, port)]);
    }
    // RFC 6761: localhost is loopback and must not need an external resolver.
    if host.strip_suffix('.').unwrap_or(host).eq_ignore_ascii_case("localhost") {
        return Ok(vec![
            SocketAddr::new(Ipv6Addr::LOCALHOST.into(), port),
            SocketAddr::new(Ipv4Addr::LOCALHOST.into(), port),
        ]);
    }
    let host = host.to_owned();
    blocking(move || (host.as_str(), port).to_socket_addrs().map(|addrs| addrs.collect())).await
}

fn slots() -> Arc<Semaphore> {
    static SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();
    SLOTS.get_or_init(|| Arc::new(Semaphore::new(LIMIT))).clone()
}

async fn blocking<T: Send + 'static>(job: impl FnOnce() -> io::Result<T> + Send + 'static) -> io::Result<T> {
    let permit = slots().acquire_owned().await.map_err(io::Error::other)?;
    tokio::task::spawn_blocking(move || {
        // Keep the permit inside the OS operation even if its waiting future ends.
        let _permit = permit;
        job()
    })
    .await
    .map_err(io::Error::other)?
}

#[cfg(test)]
#[path = "../../../tests/core/unit/dns.rs"]
mod tests;
