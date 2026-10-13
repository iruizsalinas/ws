//! Event envelopes retain admission slots until the application drains them.

use crate::{Event, lock};
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
}

#[derive(Clone)]
pub(crate) struct Events {
    tx: Sender<Queued>,
    permit: Arc<Mutex<Option<Arc<Permit>>>>,
}

impl From<Sender<Queued>> for Events {
    fn from(tx: Sender<Queued>) -> Self {
        Self { tx, permit: Arc::default() }
    }
}

impl Events {
    pub fn admitted(&self, permit: Arc<Permit>) -> Self {
        Self { tx: self.tx.clone(), permit: Arc::new(Mutex::new(Some(permit))) }
    }

    pub fn send(&self, event: Event) -> Result<(), ()> {
        let mut permit = lock(&self.permit);
        // A retained closed Conn must not retain capacity forever. The final
        // event and the network worker still own their slots until drained.
        let slot = if matches!(event, Event::Close { .. }) { permit.take() } else { permit.clone() };
        self.tx.send(Queued { event, _permit: slot }).map_err(|_| ())
    }
}

#[cfg(test)]
#[path = "../tests/unit/queue.rs"]
mod tests;
