//! Application-visible state and ordered event emission.
use super::{Link, State};
use crate::{Event, Id, lock};
use std::sync::atomic::Ordering::Relaxed;

impl Link {
    pub fn id(&self) -> Id {
        self.id
    }
    pub fn state(&self) -> State {
        lock(&self.flags).state
    }
    pub fn protocol(&self) -> Option<&str> {
        self.protocol.get().and_then(|p| p.as_deref())
    }
    pub fn buffered(&self) -> usize {
        if self.state() == State::Closed { 0 } else { self.buffered.load(Relaxed) }
    }

    pub fn fail(&self, error: &str) {
        self.emit(Event::Error { id: self.id, error: error.to_string() });
        self.finish(1006, String::new());
    }

    pub(super) fn emit(&self, event: Event) {
        let sent = {
            let f = lock(&self.flags);
            f.state == State::Closed || self.events.send(event).is_ok()
        };
        if !sent || self.events.closed() {
            self.terminate();
        }
    }
}
