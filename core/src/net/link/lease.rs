//! Dropped or panicked connection futures cannot leave an open retained handle.
use super::Link;
use std::sync::Arc;

pub(in crate::net) struct Lease(Arc<Link>);

impl Link {
    pub(in crate::net) fn lease(self: &Arc<Self>) -> Lease {
        Lease(self.clone())
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.0.terminate();
    }
}
