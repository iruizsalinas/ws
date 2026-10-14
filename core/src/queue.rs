//! Event envelopes retain admission slots until the application drains them.

use crate::{Event, lock, signal::Signal};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

pub(crate) struct Permit(Arc<AtomicUsize>);

impl Permit {
    pub fn acquire(count: &Arc<AtomicUsize>, limit: usize) -> Option<Arc<Self>> {
        let mut n = count.load(Relaxed);
        while n < limit {
            match count.compare_exchange_weak(n, n + 1, Relaxed, Relaxed) {
                Ok(_) => return Some(Arc::new(Self(count.clone()))),
                Err(current) => n = current,
            }
        }
        None
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Relaxed);
    }
}

pub(crate) struct Queued {
    pub event: Event,
    _permit: Option<Arc<Permit>>,
    _pending: Option<Arc<Permit>>,
}

type Admission = Arc<Mutex<Option<Arc<Permit>>>>;

#[derive(Clone)]
pub(crate) struct Events {
    tx: Sender<Queued>,
    // Client/hub senders have no admission slot and need no shared slot lock.
    permit: Option<Admission>,
    /// Set once the hub is gone; events sent after that are never read.
    closed: Arc<Signal>,
}

impl From<Sender<Queued>> for Events {
    fn from(tx: Sender<Queued>) -> Self {
        Self { tx, permit: None, closed: Arc::default() }
    }
}

impl Events {
    pub fn admitted(&self, permit: Arc<Permit>) -> Self {
        Self { tx: self.tx.clone(), permit: Some(Arc::new(Mutex::new(Some(permit)))), closed: self.closed.clone() }
    }

    pub fn close(&self) {
        self.closed.set();
    }

    pub fn closed(&self) -> bool {
        self.closed.is_set()
    }

    pub async fn wait_closed(&self) {
        self.closed.wait().await;
    }

    pub fn send(&self, event: Event) -> Result<(), ()> {
        self.send_pending(event, None)
    }

    /// A request retains its handshake slot even if its worker times out.
    pub fn send_pending(&self, event: Event, pending: Option<Arc<Permit>>) -> Result<(), ()> {
        // A retained closed Conn must not retain capacity forever. The final
        // event and the network worker still own their slots until drained.
        let slot = self.permit.as_ref().and_then(|admission| {
            let mut permit = lock(admission);
            if matches!(event, Event::Close { .. }) { permit.take() } else { permit.clone() }
        });
        if self.closed() {
            return Err(());
        }
        self.tx.send(Queued { event, _permit: slot, _pending: pending }).map_err(|_| ())
    }
}

#[cfg(test)]
#[path = "../../tests/core/unit/queue.rs"]
mod tests;
