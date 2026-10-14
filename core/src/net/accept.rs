//! Admission happens before spawning a handshake task.
use super::server::Shared;
use crate::queue::Permit;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;

const MAX_PENDING: usize = 256;

pub(super) async fn accept(listener: TcpListener, shared: Arc<Shared>) {
    loop {
        let accepted = tokio::select! {
            biased;
            _ = shared.stop.wait() => return,
            _ = shared.events.wait_closed() => return,
            tcp = listener.accept() => tcp,
        };
        let Ok((tcp, _)) = accepted else {
            tokio::select! {
                _ = shared.stop.wait() => return,
                _ = shared.events.wait_closed() => return,
                _ = tokio::time::sleep(Duration::from_millis(50)) => {}
            }
            continue;
        };
        let Some(permit) = Permit::acquire(&shared.connections, shared.max_connections) else {
            continue;
        };
        let Some(pending) = Permit::acquire(&shared.pending, MAX_PENDING) else {
            continue;
        };
        tokio::spawn(super::serve::serve(shared.clone(), tcp, permit, pending));
    }
}
