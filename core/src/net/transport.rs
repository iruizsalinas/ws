//! TLS verification and record ordering stay owned by rustls.
use super::activity::{Activity, Observed};
use super::stream::Stream;
use super::tls_stream::Tls;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ServerConfig};
use std::io;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::Notify;
use tokio_rustls::{TlsAcceptor, TlsConnector};

fn socket(tcp: TcpStream) -> io::Result<(Observed<TcpStream>, std::net::TcpStream)> {
    // One descriptor is retained solely for immediate synchronous termination.
    let tcp = tcp.into_std()?;
    tcp.set_nodelay(true)?;
    let stop = tcp.try_clone()?;
    Ok((Observed { io: TcpStream::from_std(tcp)?, activity: Arc::new(Activity::default()) }, stop))
}

pub(super) async fn client(
    tcp: TcpStream,
    tls: Option<(Arc<ClientConfig>, ServerName<'static>)>,
) -> io::Result<Stream> {
    let (tcp, stop) = socket(tcp)?;
    let activity = tcp.activity.clone();
    let flush = Arc::new(Notify::new());
    let chunk = if tls.is_some() { 16 * 1024 } else { 64 * 1024 };
    let io: super::stream::Transport = match tls {
        Some((config, name)) => {
            Box::new(Tls { io: TlsConnector::from(config).connect(name, tcp).await?.into(), flush: flush.clone() })
        }
        None => Box::new(tcp),
    };
    Ok(Stream { tcp: stop, io, activity, flush, chunk })
}

pub(super) async fn server(tcp: TcpStream, tls: Option<Arc<ServerConfig>>) -> io::Result<Stream> {
    let (tcp, stop) = socket(tcp)?;
    let activity = tcp.activity.clone();
    let flush = Arc::new(Notify::new());
    let chunk = if tls.is_some() { 16 * 1024 } else { 64 * 1024 };
    let io: super::stream::Transport = match tls {
        Some(config) => Box::new(Tls { io: TlsAcceptor::from(config).accept(tcp).await?.into(), flush: flush.clone() }),
        None => Box::new(tcp),
    };
    Ok(Stream { tcp: stop, io, activity, flush, chunk })
}
