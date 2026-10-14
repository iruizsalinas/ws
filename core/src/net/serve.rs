//! A server handshake is cancellable without occupying a worker thread.
use super::{link::Link, runtime, server::Shared, stream::HeadError, stream::Stream, transport};
use crate::proto::handshake;
use crate::queue::Permit;
use crate::{Conn, Event, Reply, Request};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
type Accepted = (Stream, Vec<u8>, Option<String>, Request);

pub(super) async fn serve(shared: Arc<Shared>, tcp: TcpStream, permit: Arc<Permit>, pending: Arc<Permit>) {
    let events = shared.events.admitted(permit.clone());
    let accepted = tokio::select! {
        biased;
        _ = shared.stop.wait() => return,
        _ = events.wait_closed() => return,
        accepted = handshake(&shared, &events, tcp, pending.clone()) => accepted,
    };
    let Some((stream, rest, protocol, req)) = accepted else {
        return;
    };
    if shared.stop.is_set() || events.closed() {
        return;
    }
    drop(pending);
    let link = Link::new(crate::next_id(), true, shared.config, events);
    let open = Event::Connection { server: shared.id, conn: Conn(link.clone()), req };
    link.start(stream, rest, protocol, open).await;
}

async fn handshake(
    shared: &Shared,
    events: &crate::queue::Events,
    tcp: TcpStream,
    pending: Arc<Permit>,
) -> Option<Accepted> {
    let addr = tcp.peer_addr().ok()?;
    let deadline = Instant::now() + HANDSHAKE_TIMEOUT;
    let mut stream = runtime::until(deadline, transport::server(tcp, shared.tls.clone())).await.ok()?;
    let head = tokio::time::timeout_at(deadline.into(), stream.read_head()).await;
    let (head, rest) = match head {
        Ok(Ok(head)) => head,
        Ok(Err(HeadError::TooLarge)) => {
            reject(&mut stream, 431).await;
            return None;
        }
        Err(_) => {
            reject(&mut stream, 408).await;
            return None;
        }
        Ok(Err(HeadError::Io(_))) => return None,
    };
    let parsed = match handshake::request(&head) {
        Ok(parsed) => parsed,
        Err(status) => {
            reject(&mut stream, status).await;
            return None;
        }
    };
    let req = Request { path: parsed.target, headers: parsed.headers, protocols: parsed.protocols, addr };
    let (reply, decision) = crate::reply::channel();
    events.send_pending(Event::Request { server: shared.id, req: req.clone(), reply }, Some(pending)).ok()?;
    let protocol = match tokio::time::timeout_at(deadline.into(), decision).await {
        Ok(Ok(Reply::Accept(protocol))) => {
            if protocol.as_ref().is_some_and(|p| !req.protocols.contains(p)) {
                reject(&mut stream, 500).await;
                return None;
            }
            protocol
        }
        Ok(Ok(Reply::Reject(status))) => {
            reject(&mut stream, if (400..=599).contains(&status) { status } else { 500 }).await;
            return None;
        }
        _ => {
            reject(&mut stream, 503).await;
            return None;
        }
    };
    runtime::until(deadline, stream.write_all(&handshake::switching(&parsed.key, protocol.as_deref()))).await.ok()?;
    Some((stream, rest, protocol, req))
}

async fn reject(stream: &mut Stream, status: u16) {
    let deadline = Instant::now() + Duration::from_secs(1);
    if runtime::until(deadline, stream.write_all(&handshake::reject(status))).await.is_err() {
        return;
    }
    if runtime::until(deadline, stream.io.shutdown()).await.is_err() {
        return;
    }
    // Drain briefly so closing with unread input doesn't reset the HTTP response.
    let mut buf = [0; 4096];
    let mut total = 0;
    while total < 64 * 1024 {
        match runtime::until(deadline, stream.io.read(&mut buf)).await {
            Ok(0) | Err(_) => break,
            Ok(n) => total += n,
        }
    }
}
