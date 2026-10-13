//! Outgoing connections.

use super::dial::dial;
use super::link::Link;
use super::stream::{HeadError, Stream};
use crate::proto::{base64, handshake, http, url};
use crate::{Event, Headers, Id, MAX_PAYLOAD, State, random, tls};
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, Connection};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

pub struct Options {
    pub protocols: Vec<String>,
    pub headers: Headers,
    /// TLS settings for `wss://`; Mozilla's roots when `None`.
    pub tls: Option<Arc<ClientConfig>>,
    pub timeout: Duration,
    pub max_payload: usize,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            protocols: Vec::new(),
            headers: Vec::new(),
            tls: None,
            timeout: Duration::from_secs(10),
            max_payload: MAX_PAYLOAD,
        }
    }
}

struct Target {
    url: url::Url,
    tls: Option<(Arc<ClientConfig>, ServerName<'static>)>,
}

pub(crate) fn connect(events: Sender<Event>, id: Id, input: &str, opts: Options) -> Result<Arc<Link>, String> {
    let url = url::parse(input)?;
    for (i, p) in opts.protocols.iter().enumerate() {
        if !http::is_token(p) || opts.protocols[..i].contains(p) {
            return Err(format!("invalid subprotocol: {p}"));
        }
    }
    for (name, value) in &opts.headers {
        http::check_header(name, value)?;
    }
    let tls = match url.tls {
        true => {
            let config = match &opts.tls {
                Some(c) => c.clone(),
                None => tls::client(None)?,
            };
            let name = ServerName::try_from(url.host.clone()).map_err(|_| "invalid URL host")?;
            Some((config, name))
        }
        false => None,
    };
    let link = Link::new(id, false, opts.max_payload, events);
    let target = Target { url, tls };
    let worker = link.clone();
    std::thread::Builder::new()
        .name("ws-client".into())
        .spawn(move || match establish(&worker, &target, &opts) {
            Ok((tcp, tls, rest, protocol)) => worker.start(tcp, tls, rest, protocol, Event::Open { id }),
            Err(e) => worker.fail(&e),
        })
        .map_err(|e| format!("cannot start connection thread: {e}"))?;
    Ok(link)
}

type Established = (TcpStream, Option<Connection>, Vec<u8>, Option<String>);

fn establish(link: &Link, target: &Target, opts: &Options) -> Result<Established, String> {
    let deadline = Instant::now() + opts.timeout;
    let alive = || link.state() == State::Connecting;
    let url = &target.url;
    let addrs =
        (url.host.as_str(), url.port).to_socket_addrs().map_err(|e| format!("cannot resolve {}: {e}", url.host))?;
    let tcp =
        dial(addrs.collect(), deadline).map_err(|e| e.unwrap_or_else(|| format!("cannot resolve {}", url.host)))?;
    let tls = match &target.tls {
        Some((config, name)) => {
            let conn =
                ClientConnection::new(config.clone(), name.clone()).map_err(|e| format!("TLS setup failed: {e}"))?;
            Some(Connection::from(conn))
        }
        None => None,
    };
    let mut s = Stream::new(tcp, tls, deadline, &alive);
    s.handshake().map_err(|e| format!("TLS handshake failed: {e}"))?;
    let key = base64::encode(&random::<16>());
    s.write_all(&handshake::client_request(url, &key, &opts.protocols, &opts.headers))
        .map_err(|e| format!("handshake failed: {e}"))?;
    let (head, rest) = s.read_head().map_err(|e| match e {
        HeadError::TooLarge => "handshake response too large".to_string(),
        HeadError::Io(e) => format!("handshake failed: {e}"),
    })?;
    let protocol = handshake::response(&head, &key, &opts.protocols)?;
    Ok((s.tcp, s.tls, rest, protocol))
}
